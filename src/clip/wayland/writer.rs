use std::io::{ErrorKind, PipeWriter, Write};
use std::sync::Arc;

const CHUNK: usize = 64 * 1024;
const BURST: usize = 4 * 1024 * 1024;

pub struct Outgoing {
    body: Arc<[u8]>,
    offset: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    Writing,
    Finished,
    Abandoned,
}

impl Outgoing {
    pub fn new(body: Arc<[u8]>) -> Self {
        Self { body, offset: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.body.len().saturating_sub(self.offset)
    }

    pub fn pump(&mut self, writer: &PipeWriter) -> Progress {
        let mut sent = 0;

        while self.offset < self.body.len() && sent < BURST {
            let end = (self.offset + CHUNK).min(self.body.len());

            match (&mut &*writer).write(&self.body[self.offset..end]) {
                Ok(0) => return Progress::Abandoned,
                Ok(written) => {
                    self.offset += written;
                    sent += written;
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Progress::Writing,
                Err(error) => {
                    if error.kind() != ErrorKind::BrokenPipe {
                        tracing::debug!(%error, "a clipboard receiver went away mid-transfer");
                    }
                    return Progress::Abandoned;
                }
            }
        }

        if self.offset >= self.body.len() {
            Progress::Finished
        } else {
            Progress::Writing
        }
    }
}

pub fn set_nonblocking(writer: &PipeWriter) {
    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};

    let result =
        fcntl_getfl(writer).and_then(|flags| fcntl_setfl(writer, flags | OFlags::NONBLOCK));

    if let Err(error) = result {
        tracing::debug!(%error, "could not make a clipboard write end non-blocking");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn a_short_body_goes_out_in_one_pump() {
        let (mut reader, writer) = std::io::pipe().unwrap();
        let mut outgoing = Outgoing::new(Arc::from(&b"hello"[..]));

        assert_eq!(outgoing.pump(&writer), Progress::Finished);
        drop(writer);

        let mut received = String::new();
        reader.read_to_string(&mut received).unwrap();
        assert_eq!(received, "hello");
    }

    #[test]
    fn a_long_body_takes_several_pumps() {
        let payload: Arc<[u8]> = vec![b'z'; BURST + CHUNK].into();
        let (mut reader, writer) = std::io::pipe().unwrap();
        set_nonblocking(&writer);
        let mut outgoing = Outgoing::new(Arc::clone(&payload));

        let received = std::thread::spawn(move || {
            let mut buffer = Vec::new();
            reader.read_to_end(&mut buffer).unwrap();
            buffer
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut pumps = 0;
        loop {
            match outgoing.pump(&writer) {
                Progress::Finished => break,
                Progress::Writing => {
                    pumps += 1;
                    assert!(
                        std::time::Instant::now() < deadline,
                        "transfer timed out with {} bytes remaining",
                        outgoing.remaining()
                    );
                    std::thread::yield_now();
                }
                Progress::Abandoned => panic!("the receiver stayed open"),
            }
        }
        drop(writer);

        assert_eq!(received.join().unwrap().len(), payload.len());
        assert!(
            pumps >= 1,
            "a body larger than one burst needs another pump"
        );
    }

    #[test]
    fn an_empty_body_is_finished_before_it_starts() {
        let (_reader, writer) = std::io::pipe().unwrap();
        let mut outgoing = Outgoing::new(Arc::from([]));

        assert_eq!(outgoing.pump(&writer), Progress::Finished);
        assert_eq!(outgoing.remaining(), 0);
    }

    #[test]
    fn a_receiver_that_leaves_early_is_not_an_error() {
        let (reader, writer) = std::io::pipe().unwrap();
        set_nonblocking(&writer);
        drop(reader);

        let mut outgoing = Outgoing::new(Arc::from(vec![b'q'; CHUNK * 2]));

        let mut outcome = outgoing.pump(&writer);
        for _ in 0..8 {
            if outcome != Progress::Writing {
                break;
            }
            outcome = outgoing.pump(&writer);
        }

        assert_eq!(outcome, Progress::Abandoned);
    }
}
