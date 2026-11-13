//! Big clipboards (images, copied files, long text) go over a TCP connection of their own, so a
//! screenshot or a folder never holds up the cursor and keys on the UDP link. The receiver
//! listens on the same port number as its UDP socket. Everything after a short header is sealed
//! with the session's bulk key for that direction, in frames numbered from zero, and the last
//! frame says it is the last, so a cut-off transfer is never taken for a whole one.

use std::fs;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};

const MAGIC: &[u8; 4] = b"RBK1";
const FRAME: usize = 64 * 1024;
/// The most one copy may hold, all its files together.
pub const MOST: u64 = 4 * 1024 * 1024 * 1024;
const MOST_ENTRIES: u32 = 50_000;
const MOST_NAME: usize = 4096;
/// The most an image or text may take.
const MOST_BLOB: u64 = 512 * 1024 * 1024;

const TEXT: u8 = 1;
const IMAGE: u8 = 2;
const FILES: u8 = 3;
const FILE: u8 = 0;
const FOLDER: u8 = 1;

/// What goes over.
pub enum Content {
    Text(String),
    /// A PNG.
    Image(Vec<u8>),
    /// Files and folders, each with everything in it.
    Files(Vec<PathBuf>),
}

/// What came over. Files are already on disk, in a folder of their own.
#[derive(Debug, PartialEq)]
pub enum Arrived {
    Text(String),
    Image(Vec<u8>),
    Files(Vec<PathBuf>),
}

/// The clear part at the start of a transfer: which session it belongs to, and its number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hello {
    pub session: u32,
    pub id: u32,
}

impl Hello {
    pub fn read(input: &mut impl Read) -> io::Result<Hello> {
        let mut header = [0u8; 12];
        input.read_exact(&mut header)?;
        if &header[..4] != MAGIC {
            return Err(invalid("not a Roam transfer"));
        }
        let word = |at: usize| u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]]);
        Ok(Hello { session: word(4), id: word(8) })
    }

    fn write(&self, out: &mut impl Write) -> io::Result<()> {
        out.write_all(MAGIC)?;
        out.write_all(&self.session.to_le_bytes())?;
        out.write_all(&self.id.to_le_bytes())
    }
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.to_owned())
}

fn nonce(id: u32, count: u64) -> Nonce {
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(&id.to_le_bytes());
    nonce[4..].copy_from_slice(&count.to_le_bytes());
    nonce.into()
}

/// Seals what is written into numbered frames.
struct Sealer<W: Write> {
    out: W,
    aead: ChaCha20Poly1305,
    id: u32,
    count: u64,
    buffer: Vec<u8>,
}

impl<W: Write> Sealer<W> {
    fn new(out: W, key: [u8; 32], id: u32) -> Self {
        Self { out, aead: ChaCha20Poly1305::new(&key.into()), id, count: 0, buffer: Vec::with_capacity(FRAME + 1) }
    }

    fn frame(&mut self, last: bool) -> io::Result<()> {
        let mut plain = Vec::with_capacity(self.buffer.len() + 1);
        plain.push(u8::from(last));
        plain.append(&mut self.buffer);
        let sealed = self
            .aead
            .encrypt(&nonce(self.id, self.count), Payload { msg: &plain, aad: MAGIC })
            .map_err(|_| io::Error::other("couldn't seal"))?;
        self.count += 1;
        self.out.write_all(&(sealed.len() as u32).to_le_bytes())?;
        self.out.write_all(&sealed)
    }

    fn finish(mut self) -> io::Result<()> {
        self.frame(true)?;
        self.out.flush()
    }
}

impl<W: Write> Write for Sealer<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let taken = data.len().min(FRAME - self.buffer.len());
        self.buffer.extend_from_slice(&data[..taken]);
        if self.buffer.len() == FRAME {
            self.frame(false)?;
        }
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Opens numbered frames, and reads what was in them.
struct Opener<R: Read> {
    input: R,
    aead: ChaCha20Poly1305,
    id: u32,
    count: u64,
    plain: Vec<u8>,
    at: usize,
    ended: bool,
}

impl<R: Read> Opener<R> {
    fn new(input: R, key: [u8; 32], id: u32) -> Self {
        Self { input, aead: ChaCha20Poly1305::new(&key.into()), id, count: 0, plain: Vec::new(), at: 0, ended: false }
    }

    fn next_frame(&mut self) -> io::Result<()> {
        let mut length = [0u8; 4];
        self.input.read_exact(&mut length)?;
        let length = u32::from_le_bytes(length) as usize;
        if length > FRAME + 64 {
            return Err(invalid("a frame too big"));
        }
        let mut sealed = vec![0u8; length];
        self.input.read_exact(&mut sealed)?;
        let plain = self
            .aead
            .decrypt(&nonce(self.id, self.count), Payload { msg: &sealed, aad: MAGIC })
            .map_err(|_| invalid("a frame that isn't genuine"))?;
        let Some(&last) = plain.first() else { return Err(invalid("an empty frame")) };
        self.count += 1;
        self.ended = last == 1;
        self.plain = plain;
        self.at = 1;
        Ok(())
    }

    /// Whether everything was read, up to and including the last frame.
    fn done(&mut self) -> bool {
        self.ended && self.at == self.plain.len()
    }
}

impl<R: Read> Read for Opener<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.at == self.plain.len() {
            if self.ended {
                return Ok(0);
            }
            self.next_frame()?;
        }
        let taken = buf.len().min(self.plain.len() - self.at);
        buf[..taken].copy_from_slice(&self.plain[self.at..self.at + taken]);
        self.at += taken;
        Ok(taken)
    }
}

struct Entry {
    name: String,
    folder: bool,
    size: u64,
    source: PathBuf,
}

/// Everything in `paths`, folders first and then what is in them. Links are left out.
fn walk(paths: &[PathBuf]) -> io::Result<Vec<Entry>> {
    fn add(entries: &mut Vec<Entry>, path: &Path, name: String, total: &mut u64) -> io::Result<()> {
        let meta = fs::symlink_metadata(path)?;
        if meta.is_dir() {
            entries.push(Entry { name: name.clone(), folder: true, size: 0, source: path.to_owned() });
            let mut children: Vec<PathBuf> = fs::read_dir(path)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
            children.sort();
            for child in children {
                let Some(file) = child.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
                add(entries, &child, format!("{name}/{file}"), total)?;
            }
        } else if meta.is_file() {
            *total += meta.len();
            entries.push(Entry { name, folder: false, size: meta.len(), source: path.to_owned() });
        }
        if *total > MOST || entries.len() > MOST_ENTRIES as usize {
            return Err(io::Error::other("too much to send"));
        }
        Ok(())
    }
    let mut entries = Vec::new();
    let mut total = 0;
    for path in paths {
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
        add(&mut entries, path, name, &mut total)?;
    }
    Ok(entries)
}

/// Sends `content` to `to` (its UDP address: the port number is the same).
pub fn send(to: SocketAddr, hello: Hello, key: [u8; 32], content: &Content) -> io::Result<()> {
    let stream = TcpStream::connect_timeout(&to, Duration::from_secs(3))?;
    let _ = stream.set_nodelay(true);
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut out = BufWriter::with_capacity(FRAME * 2, stream);
    hello.write(&mut out)?;
    let mut sealer = Sealer::new(out, key, hello.id);
    write_content(&mut sealer, content)?;
    sealer.finish()
}

fn write_content(out: &mut impl Write, content: &Content) -> io::Result<()> {
    match content {
        Content::Text(text) => write_blob(out, TEXT, text.as_bytes()),
        Content::Image(png) => write_blob(out, IMAGE, png),
        Content::Files(paths) => {
            let entries = walk(paths)?;
            if entries.is_empty() {
                return Err(io::Error::other("nothing to send"));
            }
            out.write_all(&[FILES])?;
            out.write_all(&(entries.len() as u32).to_le_bytes())?;
            for entry in &entries {
                out.write_all(&[if entry.folder { FOLDER } else { FILE }])?;
                out.write_all(&(entry.name.len() as u16).to_le_bytes())?;
                out.write_all(entry.name.as_bytes())?;
                out.write_all(&entry.size.to_le_bytes())?;
            }
            for entry in entries.iter().filter(|e| !e.folder) {
                let copied = io::copy(&mut fs::File::open(&entry.source)?.take(entry.size), out)?;
                if copied != entry.size {
                    return Err(io::Error::other("a file changed while it was sent"));
                }
            }
            Ok(())
        }
    }
}

fn write_blob(out: &mut impl Write, kind: u8, bytes: &[u8]) -> io::Result<()> {
    out.write_all(&[kind])?;
    out.write_all(&(bytes.len() as u64).to_le_bytes())?;
    out.write_all(bytes)
}

/// Receives a transfer whose `hello` was already read, with the sender's bulk key. Files land in
/// a new folder in `inbox`.
pub fn receive(stream: impl Read, hello: Hello, key: [u8; 32], inbox: &Path) -> io::Result<Arrived> {
    let mut input = Opener::new(BufReader::with_capacity(FRAME * 2, stream), key, hello.id);
    let mut kind = [0u8; 1];
    input.read_exact(&mut kind)?;
    let arrived = match kind[0] {
        TEXT => Arrived::Text(String::from_utf8(read_blob(&mut input)?).map_err(|_| invalid("text that isn't"))?),
        IMAGE => Arrived::Image(read_blob(&mut input)?),
        FILES => {
            let folder = inbox.join(format!("{:08x}-{}", hello.id, crate::crypto::hex(&crate::crypto::random::<4>())));
            match read_files(&mut input, &folder) {
                Ok(paths) => Arrived::Files(paths),
                Err(err) => {
                    let _ = fs::remove_dir_all(&folder);
                    return Err(err);
                }
            }
        }
        _ => return Err(invalid("an unknown kind of transfer")),
    };
    let mut rest = [0u8; 1];
    if input.read(&mut rest)? != 0 || !input.done() {
        return Err(invalid("more than was announced"));
    }
    Ok(arrived)
}

fn u64_of(input: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0u8; 8];
    input.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_blob(input: &mut impl Read) -> io::Result<Vec<u8>> {
    let length = u64_of(input)?;
    if length > MOST_BLOB {
        return Err(invalid("too big"));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    input.take(length).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != length {
        return Err(invalid("cut short"));
    }
    Ok(bytes)
}

/// A name from the other computer, as a path inside `folder`, or `None` if it would reach
/// outside it. Characters Windows can't have in names become `_`.
fn place(folder: &Path, name: &str) -> Option<PathBuf> {
    let mut path = folder.to_owned();
    for part in name.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        let clean: String = part
            .chars()
            .map(
                |c| if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*') { '_' } else { c },
            )
            .collect();
        path.push(clean);
    }
    Some(path)
}

fn read_files(input: &mut impl Read, folder: &Path) -> io::Result<Vec<PathBuf>> {
    let mut count = [0u8; 4];
    input.read_exact(&mut count)?;
    let count = u32::from_le_bytes(count);
    if count == 0 || count > MOST_ENTRIES {
        return Err(invalid("too many files"));
    }
    let mut entries = Vec::with_capacity(count as usize);
    let mut total = 0u64;
    for _ in 0..count {
        let mut head = [0u8; 3];
        input.read_exact(&mut head)?;
        let length = usize::from(u16::from_le_bytes([head[1], head[2]]));
        if length > MOST_NAME {
            return Err(invalid("a name too long"));
        }
        let mut name = vec![0u8; length];
        input.read_exact(&mut name)?;
        let name = String::from_utf8(name).map_err(|_| invalid("a name that isn't text"))?;
        let size = u64_of(input)?;
        total = total.saturating_add(size);
        if total > MOST {
            return Err(invalid("too big"));
        }
        let path = place(folder, &name).ok_or_else(|| invalid("a name outside the folder"))?;
        entries.push((path, head[0] == FOLDER, size, !name.contains('/')));
    }
    fs::create_dir_all(folder)?;
    let mut top = Vec::new();
    for (path, is_folder, _, is_top) in &entries {
        if *is_folder {
            fs::create_dir_all(path)?;
        }
        if *is_top && !top.contains(path) {
            top.push(path.clone());
        }
    }
    for (path, _, size, _) in entries.iter().filter(|e| !e.1) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = BufWriter::new(fs::File::create(path)?);
        let copied = io::copy(&mut input.take(*size), &mut file)?;
        file.flush()?;
        if copied != *size {
            return Err(invalid("cut short"));
        }
    }
    Ok(top)
}

/// Keeps the newest `keep` transfers in `inbox` and deletes the rest.
pub fn tidy(inbox: &Path, keep: usize) {
    let Ok(read) = fs::read_dir(inbox) else { return };
    let mut folders: Vec<(std::time::SystemTime, PathBuf)> = read
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()))
        .collect();
    folders.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, path) in folders.into_iter().skip(keep) {
        let _ = fs::remove_dir_all(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir()
            .join(format!("roam-bulk-{name}-{}", crate::crypto::hex(&crate::crypto::random::<4>())));
        fs::create_dir_all(&path).unwrap();
        path
    }

    /// Sends `content` over a real TCP connection and returns what arrived.
    fn round_trip(content: Content, inbox: &Path) -> io::Result<Arrived> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let to = listener.local_addr().unwrap();
        let key = [7u8; 32];
        let hello = Hello { session: 5, id: 9 };
        let sender = std::thread::spawn(move || send(to, hello, key, &content));
        let (mut stream, _) = listener.accept().unwrap();
        let heard = Hello::read(&mut stream)?;
        assert_eq!(heard, hello);
        let arrived = receive(stream, heard, key, inbox);
        sender.join().unwrap()?;
        arrived
    }

    #[test]
    fn an_image_and_text_arrive_whole() {
        let inbox = folder("blob");
        let png: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        assert_eq!(round_trip(Content::Image(png.clone()), &inbox).unwrap(), Arrived::Image(png));
        let text = "héllo ".repeat(40_000);
        assert_eq!(round_trip(Content::Text(text.clone()), &inbox).unwrap(), Arrived::Text(text));
        let _ = fs::remove_dir_all(inbox);
    }

    #[test]
    fn files_and_folders_arrive_with_everything_in_them() {
        let source = folder("source");
        fs::write(source.join("notes.txt"), b"one").unwrap();
        fs::create_dir_all(source.join("photos/2026")).unwrap();
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 13) as u8).collect();
        fs::write(source.join("photos/2026/big.raw"), &big).unwrap();
        fs::write(source.join("photos/a:b.txt"), b"odd name").unwrap();
        fs::create_dir_all(source.join("photos/empty")).unwrap();
        let inbox = folder("inbox");
        let content = Content::Files(vec![source.join("notes.txt"), source.join("photos")]);
        let Arrived::Files(top) = round_trip(content, &inbox).unwrap() else { panic!() };
        assert_eq!(top.len(), 2);
        assert_eq!(fs::read(&top[0]).unwrap(), b"one");
        assert!(top[0].ends_with("notes.txt"));
        assert_eq!(fs::read(top[1].join("2026/big.raw")).unwrap(), big);
        assert_eq!(fs::read(top[1].join("a_b.txt")).unwrap(), b"odd name");
        assert!(top[1].join("empty").is_dir());
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(inbox);
    }

    #[test]
    fn names_never_reach_outside_the_folder() {
        let folder = Path::new("/inbox/1");
        assert_eq!(place(folder, "a/b.txt"), Some(folder.join("a").join("b.txt")));
        assert_eq!(place(folder, "../x"), None);
        assert_eq!(place(folder, "a/../../x"), None);
        assert_eq!(place(folder, "/etc/passwd"), None);
        assert_eq!(place(folder, "a\\..\\x"), Some(folder.join("a_.._x")));
        assert_eq!(place(folder, "C:x"), Some(folder.join("C_x")));
    }

    #[test]
    fn a_wrong_key_or_a_cut_off_transfer_is_refused() {
        let mut sealed = Vec::new();
        let mut sealer = Sealer::new(&mut sealed, [1; 32], 3);
        write_content(&mut sealer, &Content::Text("secret".into())).unwrap();
        sealer.finish().unwrap();
        let inbox = folder("refused");
        let hello = Hello { session: 1, id: 3 };
        assert_eq!(receive(&sealed[..], hello, [1; 32], &inbox).unwrap(), Arrived::Text("secret".into()));
        assert!(receive(&sealed[..], hello, [2; 32], &inbox).is_err(), "another key");
        assert!(receive(&sealed[..], Hello { id: 4, ..hello }, [1; 32], &inbox).is_err(), "another number");
        let mut tampered = sealed.clone();
        tampered[10] ^= 1;
        assert!(receive(&tampered[..], hello, [1; 32], &inbox).is_err(), "changed on the way");
        assert!(receive(&sealed[..sealed.len() - 3], hello, [1; 32], &inbox).is_err(), "cut off");
        let _ = fs::remove_dir_all(inbox);
    }

    #[test]
    fn an_image_survives_the_png_both_ways() {
        let rgba: Vec<u8> = (0..64 * 48 * 4).map(|i| (i * 7 % 256) as u8).collect();
        let png = crate::input::png(64, 48, &rgba).unwrap();
        let back = crate::input::unpng(&png).unwrap();
        assert_eq!(back, crate::input::Copied::Image { width: 64, height: 48, rgba });
        assert!(crate::input::unpng(b"not a png").is_none());
    }

    #[test]
    fn tidy_keeps_the_newest() {
        let inbox = folder("tidy");
        for name in ["a", "b", "c", "d"] {
            fs::create_dir_all(inbox.join(name)).unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
        tidy(&inbox, 2);
        let mut left: Vec<String> =
            fs::read_dir(&inbox).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["c", "d"]);
        let _ = fs::remove_dir_all(inbox);
    }
}
