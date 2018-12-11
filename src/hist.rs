use std::sync::mpsc::{Sender, Receiver, channel};
use std::sync::Arc;
use std::time::{Instant, Duration, SystemTime, UNIX_EPOCH};
use std::path::PathBuf;
use std::thread::{self, JoinHandle};
use std::io;
use std::{mem, fs};

use dirs::home_dir;
use hdrhistogram::{Histogram};
use hdrhistogram::serialization::V2DeflateSerializer;
use hdrhistogram::serialization::interval_log::{IntervalLogWriterBuilder, Tag};

pub type C = u64;

pub fn nanos(d: Duration) -> u64 {
    d.as_secs() * 1_000_000_000_u64 + (d.subsec_nanos() as u64)
}

pub struct HistLog {
    series: &'static str,
    tag: &'static str,
    freq: Duration,
    last_sent: Instant,
    tx: Sender<Option<Entry>>,
    hist: Histogram<C>,
    thread: Option<Arc<thread::JoinHandle<()>>>,
}

pub struct Entry {
    pub tag: &'static str,
    pub start: SystemTime,
    pub end: SystemTime,
    pub hist: Histogram<C>,
}

impl Clone for HistLog {
    fn clone(&self) -> Self {
        let thread = self.thread.as_ref().map(|x| Arc::clone(x));
        Self {
            series: self.series.clone(),
            tag: self.tag.clone(),
            freq: self.freq.clone(),
            last_sent: Instant::now(),
            tx: self.tx.clone(),
            hist: self.hist.clone(),
            thread,
        }
    }
}

impl HistLog {
    pub fn new(series: &'static str, tag: &'static str, freq: Duration) -> Self {
        let (tx, rx) = channel();
        let mut dir = home_dir().expect("home_dir");
        dir.push("src/market-maker/var/hist");
        fs::create_dir_all(&dir).unwrap();
        let thread = Some(Arc::new(Self::scribe(series, rx, dir)));
        let last_sent = Instant::now();
        let hist = Histogram::new(3).unwrap();
        Self { series, tag, freq, last_sent, tx, hist, thread }
    }

    /// Create a new `HistLog` that will save results in a specified
    /// directory (`path`).
    pub fn with_path(
        path: &str,
        series: &'static str,
        tag: &'static str,
        freq: Duration,
    ) -> Self {
        let (tx, rx) = channel();
        let dir = PathBuf::from(path);
        // let mut dir = env::home_dir().unwrap();
        // dir.push("src/market-maker/var/hist");
        fs::create_dir_all(&dir).ok();
        let thread = Some(Arc::new(Self::scribe(series, rx, dir)));
        let last_sent = Instant::now();
        let hist = Histogram::new(3).unwrap();
        Self { series, tag, freq, last_sent, tx, hist, thread }
    }


    pub fn new_with_tag(&self, tag: &'static str) -> Self {
        Self::new(self.series, tag, self.freq)
    }

    pub fn clone_with_tag(&self, tag: &'static str) -> Self {
        let thread = self.thread.as_ref().map(|x| Arc::clone(x)).unwrap();
        assert!(self.thread.is_some(), "self.thread is {:?}", self.thread);
        let tx = self.tx.clone();
        Self {
            series: self.series,
            tag,
            freq: self.freq,
            last_sent: Instant::now(),
            tx,
            hist: self.hist.clone(),
            thread: Some(thread),
        }
    }

    pub fn clone_with_tag_and_freq(&self, tag: &'static str, freq: Duration) -> HistLog {
        let mut clone = self.clone_with_tag(tag);
        clone.freq = freq;
        clone
    }

    pub fn record(&mut self, value: u64) {
        let _ = self.hist.record(value);
    }

    /// If for some reason there was a pause in between using the struct, 
    /// this resets the internal state of both the values recorded to the
    /// `Histogram` and the value of when it last sent a `Histogram` onto
    /// the writing thread.
    /// 
    pub fn reset(&mut self) {
        self.hist.clear();
        self.last_sent = Instant::now();
    }

    fn send(&mut self, loop_time: Instant) {
        let end = SystemTime::now();
        let start = end - (loop_time - self.last_sent);
        assert!(end > start, "end <= start!");
        let mut next = Histogram::new_from(&self.hist);
        mem::swap(&mut self.hist, &mut next);
        self.tx.send(Some(Entry { tag: self.tag, start, end, hist: next })).expect("sending entry failed");
        self.last_sent = loop_time;
    }

    pub fn check_send(&mut self, loop_time: Instant) {
        //let since = loop_time - self.last_sent;
        if loop_time > self.last_sent && loop_time - self.last_sent >= self.freq {
            // send sets self.last_sent to loop_time fyi
            self.send(loop_time);
        }
    }

    fn scribe(
        series  : &'static str,
        rx      : Receiver<Option<Entry>>,
        dir     : PathBuf,
    ) -> JoinHandle<()> {
        let mut ser = V2DeflateSerializer::new();
        let start_time = SystemTime::now();
        let seconds = start_time.duration_since(UNIX_EPOCH).unwrap().as_secs();
        let path = dir.join(&format!("{}-interval-log-{}.v2z", series, seconds));
        let file = fs::File::create(&path).unwrap();
        thread::Builder::new().name(format!("mm:hist:{}", series)).spawn(move || {
            let mut buf = io::LineWriter::new(file);
            let mut wtr =
                IntervalLogWriterBuilder::new() 
                    .with_base_time(UNIX_EPOCH)
                    .with_start_time(start_time)
                    .begin_log_with(&mut buf, &mut ser)
                    .unwrap();

            loop {
                match rx.recv() { //.recv_timeout(Duration::from_millis(1)) {
                //match rx.recv_timeout(Duration::new(1, 0)) {
                    Ok(Some(Entry { tag, start, end, hist })) => {
                        wtr.write_histogram(&hist, start.duration_since(UNIX_EPOCH).unwrap(),
                                            end.duration_since(start).unwrap(), Tag::new(tag))
                            .ok();
                            //.map_err(|e| { println!("{:?}", e); e }).ok();
                    }

                    // `None` used as terminate signal from `Drop`
                    Ok(None) => break,

                    _ => {
                        thread::sleep(Duration::new(0, 0));
                    }
                }

            }
        }).unwrap()
    }
}

impl Drop for HistLog {
    fn drop(&mut self) {
        if !self.hist.is_empty() { self.send(Instant::now()) }

        if let Some(arc) = self.thread.take() {
            //println!("in Drop, strong count is {}", Arc::strong_count(&arc));
            if let Ok(thread) = Arc::try_unwrap(arc) {
                let _ = self.tx.send(None);
                let _ = thread.join();
            }
        }
    }
}
