//! Writes FLAC with flacenc, a block at a time as the samples arrive.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use flacenc::bitsink::ByteSink;
use flacenc::component::{BitRepr, Stream};
use flacenc::config::Encoder;
use flacenc::error::{Verified, Verify};
use flacenc::source::{Context, Fill, FrameBuf};

use super::decode::Shape;
use crate::shelf::video::MediaError;

pub(super) struct Writer {
    file: BufWriter<File>,
    config: Verified<Encoder>,
    stream: Stream,
    block: FrameBuf,
    context: Context,
    pending: Vec<i32>,
    channels: usize,
}

impl Writer {
    pub(super) fn create(target: &Path, shape: Shape) -> Result<Self, MediaError> {
        let config = Encoder::default().into_verified().map_err(|_| MediaError::Failed)?;
        let bits = shape.bits as usize;
        let mut stream = Stream::new(shape.rate as usize, shape.channels, bits).map_err(|_| MediaError::Unsupported)?;
        stream
            .stream_info_mut()
            .set_block_sizes(config.block_size, config.block_size)
            .map_err(|_| MediaError::Failed)?;
        let block = FrameBuf::with_size(shape.channels, config.block_size).map_err(|_| MediaError::Unsupported)?;
        let mut file = BufWriter::new(File::create(target).map_err(|_| MediaError::Failed)?);
        file.write_all(&header(&stream)?).map_err(|_| MediaError::Failed)?;
        let context = Context::new(bits, shape.channels);
        let pending = Vec::with_capacity(2 * config.block_size * shape.channels);
        Ok(Self { file, config, stream, block, context, pending, channels: shape.channels })
    }

    pub(super) fn write(&mut self, samples: &[i32]) -> Result<(), MediaError> {
        self.pending.extend_from_slice(samples);
        let whole = self.config.block_size * self.channels;
        let ready = self.pending.len() / whole * whole;
        if ready == 0 {
            return Ok(());
        }
        let blocks: Vec<i32> = self.pending.drain(..ready).collect();
        for block in blocks.chunks_exact(whole) {
            self.encode(block)?;
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<(), MediaError> {
        if !self.pending.is_empty() {
            let rest = std::mem::take(&mut self.pending);
            self.encode(&rest)?;
        }
        let size = self.config.block_size;
        let info = self.stream.stream_info_mut();
        info.set_block_sizes(size, size).map_err(|_| MediaError::Failed)?;
        info.set_md5_digest(&self.context.md5_digest());
        info.set_total_samples(self.context.total_samples());
        let header = header(&self.stream)?;
        self.file.seek(SeekFrom::Start(0)).map_err(|_| MediaError::Failed)?;
        self.file.write_all(&header).map_err(|_| MediaError::Failed)?;
        self.file.flush().map_err(|_| MediaError::Failed)
    }

    fn encode(&mut self, samples: &[i32]) -> Result<(), MediaError> {
        (&mut self.block, &mut self.context).fill_interleaved(samples).map_err(|_| MediaError::Failed)?;
        let number = self.context.current_frame_number().ok_or(MediaError::Failed)?;
        let frame = flacenc::encode_fixed_size_frame(&self.config, &self.block, number, self.stream.stream_info())
            .map_err(|_| MediaError::Failed)?;
        self.stream.stream_info_mut().update_frame_info(&frame);
        let mut sink = ByteSink::new();
        frame.write(&mut sink).map_err(|_| MediaError::Failed)?;
        self.file.write_all(sink.as_slice()).map_err(|_| MediaError::Failed)
    }
}

fn header(stream: &Stream) -> Result<Vec<u8>, MediaError> {
    let mut sink = ByteSink::new();
    stream.write(&mut sink).map_err(|_| MediaError::Failed)?;
    Ok(sink.as_slice().to_vec())
}
