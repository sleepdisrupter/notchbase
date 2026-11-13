use std::cell::RefCell;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use egui::pos2;

use super::*;
use crate::annotate::rect::PixelRect;
use crate::annotate::testing::{block, line, solid};
use crate::ui::color;

const PATIENCE: Duration = Duration::from_secs(20);
const SLOW_WRITE: Duration = Duration::from_millis(30);
const PAPER: [u8; 3] = [250, 250, 250];

#[derive(Debug, PartialEq)]
enum Report {
    Done(u64),
    Failed(u64, String),
}

fn job(revision: u64, base: &Arc<Screenshot>, lines: usize) -> Job {
    let lines =
        (0..lines).map(|row| line(color::INK, 4.0, [4.0, 6.0 + 8.0 * row as f32], [28.0, 6.0 + 8.0 * row as f32]));
    Job { revision, base: base.clone(), marks: lines.map(Mark::from).collect() }
}

fn drain(jobs: Vec<Job>, write: impl FnMut(&Screenshot) -> Result<(), String>) -> Vec<Report> {
    let (sender, queue) = mpsc::channel();
    jobs.into_iter().for_each(|job| sender.send(job).expect("a queue"));
    drop(sender);
    let reports = RefCell::new(Vec::new());
    run(
        &queue,
        write,
        |revision| reports.borrow_mut().push(Report::Done(revision)),
        |revision, problem| reports.borrow_mut().push(Report::Failed(revision, problem)),
    );
    reports.into_inner()
}

#[test]
fn burst_writes_only_the_last_job() {
    let base = Arc::new(solid(32, 48, PAPER));
    let burst: Vec<Job> = (1..=5).map(|revision| job(revision, &base, revision as usize)).collect();
    let expected = composite(&base, &burst[4].marks);
    let mut written = Vec::new();
    let reports = drain(burst, |shot| {
        written.push(shot.clone());
        Ok(())
    });
    assert_eq!(reports, [Report::Done(5)]);
    assert_eq!(written.len(), 1);
    assert_eq!((written[0].width, written[0].height), (32, 48));
    assert!(written[0].rgba == expected.rgba && written[0].rgba != base.rgba);
}

#[test]
fn latest_is_the_last_queued() {
    let base = Arc::new(solid(8, 8, PAPER));
    let (sender, queue) = mpsc::channel();
    assert_eq!(latest(job(1, &base, 0), &queue).revision, 1);
    [2, 3, 4].into_iter().for_each(|revision| sender.send(job(revision, &base, 0)).expect("a queue"));
    assert_eq!(latest(job(1, &base, 0), &queue).revision, 4);
    assert!(queue.try_recv().is_err(), "the older jobs are gone");
}

#[test]
fn refusal_reported_next_job_runs() {
    let base = Arc::new(solid(8, 8, PAPER));
    let (sender, queue) = mpsc::channel();
    sender.send(job(1, &base, 1)).expect("a queue");
    let sender = RefCell::new(Some(sender));
    let reports = RefCell::new(Vec::new());
    let mut attempts = 0;
    let flaky = |_: &Screenshot| {
        attempts += 1;
        if attempts == 1 { Err("The clipboard is busy".to_owned()) } else { Ok(()) }
    };
    let done = |revision| reports.borrow_mut().push(Report::Done(revision));
    let failed = |revision, problem| {
        reports.borrow_mut().push(Report::Failed(revision, problem));
        if let Some(sender) = sender.take() {
            sender.send(job(2, &base, 1)).expect("a queue");
        }
    };
    run(&queue, flaky, done, failed);
    assert_eq!(reports.into_inner(), [Report::Failed(1, "The clipboard is busy".to_owned()), Report::Done(2)]);
}

#[test]
fn panicking_writer_is_reported() {
    let base = Arc::new(solid(8, 8, PAPER));
    let reports = drain(vec![job(3, &base, 1)], |_| panic!("the pasteboard went away"));
    assert_eq!(reports, [Report::Failed(3, CRASHED.to_owned())]);
}

#[test]
fn no_marks_copies_screenshot_as_is() {
    let base = Arc::new(solid(8, 8, PAPER));
    let mut written = Vec::new();
    let reports = drain(vec![job(9, &base, 0)], |shot| {
        written.push(shot.rgba.clone());
        Ok(())
    });
    assert_eq!((reports, written), (vec![Report::Done(9)], vec![base.rgba.clone()]));
}

#[test]
fn job_is_a_snapshot_of_the_sheet() {
    let base = Arc::new(solid(32, 32, PAPER));
    let mut sheet = Sheet::new(base.clone()).expect("a sheet");
    sheet.commit(line(color::RED, 6.0, [4.0, 4.0], [28.0, 28.0]));
    sheet.commit(line(color::GREEN, 6.0, [28.0, 4.0], [4.0, 28.0]));
    let job = Job::of(&sheet);
    sheet.undo();
    assert_eq!((job.revision, job.marks.len()), (2, 2));
    assert!(Arc::ptr_eq(&job.base, &base));
    assert_eq!(job.marks[0].line().and_then(|line| line.points().first()), Some(&pos2(4.0, 4.0)));
}

#[test]
fn job_shares_words_without_copying() {
    let mut sheet = Sheet::new(Arc::new(solid(32, 32, PAPER))).expect("a sheet");
    sheet.commit(block(color::RED, PixelRect { x: 4, y: 4, width: 20, height: 10 }));
    let masks =
        |marks: &[Mark]| marks.iter().filter_map(|mark| mark.text()).map(|words| words.coverage().as_ptr()).collect();
    let (on_the_sheet, in_the_job): (Vec<_>, Vec<_>) = (masks(sheet.marks()), masks(&Job::of(&sheet).marks));
    assert_eq!(on_the_sheet.len(), 1);
    assert_eq!(on_the_sheet, in_the_job, "the mask is the same allocation, not a copy");
}

#[test]
fn job_carries_words_to_clipboard() {
    let base = Arc::new(solid(32, 32, PAPER));
    let mut sheet = Sheet::new(base.clone()).expect("a sheet");
    sheet.commit(line(color::INK, 4.0, [2.0, 9.0], [30.0, 9.0]));
    sheet.commit(block(color::RED, PixelRect { x: 4, y: 4, width: 20, height: 10 }));
    let mut written = Vec::new();
    let reports = drain(vec![Job::of(&sheet)], |shot| {
        written.push(shot.clone());
        Ok(())
    });
    assert_eq!(reports, [Report::Done(2)]);
    let at = |x: usize, y: usize| written[0].rgba[(y * 32 + x) * 4..][..3].to_vec();
    assert_eq!((at(10, 9), at(28, 9)), (color::RED.to_array()[..3].to_vec(), color::INK.to_array()[..3].to_vec()));
    assert!(written[0].rgba == composite(&base, sheet.marks()).rgba);
}

#[test]
fn worker_writes_then_leaves_on_drop() {
    let base = Arc::new(solid(16, 16, PAPER));
    let (report, reports) = mpsc::channel();
    let (thread_name, thread_names) = mpsc::channel();
    let failure = report.clone();
    let write = move |_: &Screenshot| {
        let name = thread::current().name().map(str::to_owned);
        thread_name.send(name).map_err(|err| err.to_string())
    };
    let done = move |revision| drop(report.send(Report::Done(revision)));
    let failed = move |revision, problem| drop(failure.send(Report::Failed(revision, problem)));
    let copier = Copier::start_with(write, done, failed).expect("a worker thread");
    copier.copy(job(1, &base, 1));
    assert_eq!(reports.recv_timeout(PATIENCE), Ok(Report::Done(1)));
    assert_eq!(thread_names.try_recv(), Ok(Some(COPY_THREAD.to_owned())));
    copier.copy(job(2, &base, 2));
    drop(copier);
    assert_eq!(reports.recv_timeout(PATIENCE), Ok(Report::Done(2)), "the job queued before the drop is written");
    assert_eq!(
        reports.recv_timeout(PATIENCE),
        Err(RecvTimeoutError::Disconnected),
        "the thread is gone and took its closures with it"
    );
}

#[test]
fn finish_waits_for_queued_picture() {
    let base = Arc::new(solid(16, 16, PAPER));
    let (written, writes) = mpsc::channel();
    let write = move |_: &Screenshot| {
        thread::sleep(SLOW_WRITE);
        written.send(()).map_err(|err| err.to_string())
    };
    let copier = Copier::start_with(write, |_| {}, |_, _| {}).expect("a worker thread");
    copier.copy(job(1, &base, 1));
    copier.finish();
    assert_eq!(writes.try_recv(), Ok(()), "the write was over before finish returned");
}

#[test]
#[ignore = "writes to the real clipboard"]
fn real_clipboard_takes_the_picture() {
    let base = Arc::new(solid(64, 64, PAPER));
    let (report, reports) = mpsc::channel();
    let failure = report.clone();
    let done = move |revision| drop(report.send(Report::Done(revision)));
    let failed = move |revision, problem| drop(failure.send(Report::Failed(revision, problem)));
    let copier = Copier::start(done, failed).expect("a worker thread");
    copier.copy(job(1, &base, 3));
    assert_eq!(reports.recv_timeout(PATIENCE), Ok(Report::Done(1)));
}
