use std::io::{ErrorKind, PipeReader, PipeWriter, Read};
use std::time::Duration;

const CHUNK: usize = 64 * 1024;
const BURST: usize = 4 * 1024 * 1024;

pub const TRANSFER_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub enum Progress {
    Reading,
    Finished,
    Overflowed,
    Failed(std::io::Error),
}

pub fn pipe() -> std::io::Result<(PipeReader, PipeWriter)> {
    let (reader, writer) = std::io::pipe()?;
    set_nonblocking(&reader);
    Ok((reader, writer))
}

fn set_nonblocking(reader: &PipeReader) {
    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};

    let result =
        fcntl_getfl(reader).and_then(|flags| fcntl_setfl(reader, flags | OFlags::NONBLOCK));

    if let Err(error) = result {
        tracing::debug!(%error, "could not make the read end non-blocking");
    }
}

pub fn pump(reader: &PipeReader, buffer: &mut Vec<u8>, limit: usize) -> Progress {
    let mut taken = 0;

    loop {
        let room = limit.saturating_sub(buffer.len());
        if room == 0 {
            return Progress::Overflowed;
        }
        if taken >= BURST {
            return Progress::Reading;
        }

        let start = buffer.len();
        let wanted = CHUNK.min(room);
        buffer.resize(start + wanted, 0);

        match (&mut &*reader).read(&mut buffer[start..]) {
            Ok(0) => {
                buffer.truncate(start);
                return Progress::Finished;
            }
            Ok(read) => {
                buffer.truncate(start + read);
                taken += read;
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => buffer.truncate(start),
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                buffer.truncate(start);
                return Progress::Reading;
            }
            Err(error) => {
                buffer.truncate(start);
                return Progress::Failed(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn drain(reader: &PipeReader, limit: usize) -> (Vec<u8>, &'static str) {
        let mut buffer = Vec::new();
        loop {
            match pump(reader, &mut buffer, limit) {
                Progress::Reading => {}
                Progress::Finished => return (buffer, "finished"),
                Progress::Overflowed => return (buffer, "overflowed"),
                Progress::Failed(_) => return (buffer, "failed"),
            }
        }
    }

    #[test]
    fn a_short_write_is_read_whole() {
        let (reader, mut writer) = pipe().unwrap();
        writer.write_all(b"hello").unwrap();
        drop(writer);

        let (buffer, outcome) = drain(&reader, 1024);

        assert_eq!(buffer, b"hello");
        assert_eq!(outcome, "finished");
    }

    #[test]
    fn a_write_larger_than_one_chunk_is_reassembled() {
        let payload = vec![b'x'; CHUNK * 3 + 7];
        let (reader, mut writer) = pipe().unwrap();

        let sent = payload.clone();
        let sender = std::thread::spawn(move || {
            writer.write_all(&sent).unwrap();
        });

        let (buffer, outcome) = drain(&reader, payload.len() * 2);
        sender.join().unwrap();

        assert_eq!(buffer.len(), payload.len());
        assert_eq!(outcome, "finished");
    }

    #[test]
    fn the_ceiling_stops_the_transfer_rather_than_truncating_it() {
        let (reader, mut writer) = pipe().unwrap();
        let sender = std::thread::spawn(move || {
            let _ = writer.write_all(&vec![b'y'; CHUNK * 8]);
        });

        let (_, outcome) = drain(&reader, CHUNK * 2);
        assert_eq!(outcome, "overflowed");

        drop(reader);
        let _ = sender.join();
    }

    #[test]
    fn an_empty_selection_finishes_immediately() {
        let (reader, writer) = pipe().unwrap();
        drop(writer);

        let (buffer, outcome) = drain(&reader, 1024);

        assert!(buffer.is_empty());
        assert_eq!(outcome, "finished");
    }

    #[test]
    fn a_reader_with_no_room_left_reports_overflow_without_reading() {
        let (reader, mut writer) = pipe().unwrap();
        writer.write_all(b"data").unwrap();
        let mut buffer = vec![0; 16];

        assert!(matches!(
            pump(&reader, &mut buffer, 16),
            Progress::Overflowed
        ));
        assert_eq!(buffer.len(), 16, "the buffer is left as it was");
    }

    #[test]
    fn a_pipe_with_nothing_in_it_yet_does_not_block() {
        let (reader, _writer) = pipe().unwrap();
        let mut buffer = Vec::new();

        assert!(matches!(
            pump(&reader, &mut buffer, 1024),
            Progress::Reading
        ));
        assert!(buffer.is_empty());
    }
}
