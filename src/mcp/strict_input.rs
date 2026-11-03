//! Bounded, cancellation-safe JSON frame validation before the RMCP transport.
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, ReadBuf};

pub(super) const MAX_LINE: usize = 65536;
const CHUNK: usize = 4096;
const BUDGET: usize = 32;

pub(super) struct StrictInput<R> {
    input: R,
    chunk: [u8; CHUNK],
    pos: usize,
    end: usize,
    line: Vec<u8>,
    output_pos: usize,
    ready: bool,
    discarding: bool,
    eof: bool,
}

impl<R> StrictInput<R> {
    pub(super) fn new(input: R) -> Self {
        Self {
            input,
            chunk: [0; CHUNK],
            pos: 0,
            end: 0,
            line: Vec::new(),
            output_pos: 0,
            ready: false,
            discarding: false,
            eof: false,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for StrictInput<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if out.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        for _ in 0..BUDGET {
            if this.ready {
                let n = out.remaining().min(this.line.len() - this.output_pos);
                out.put_slice(&this.line[this.output_pos..this.output_pos + n]);
                this.output_pos += n;
                if this.output_pos == this.line.len() {
                    this.line.clear();
                    this.output_pos = 0;
                    this.ready = false;
                }
                return Poll::Ready(Ok(()));
            }
            if this.pos == this.end {
                if this.eof {
                    return Poll::Ready(Ok(()));
                }
                let mut buf = ReadBuf::new(&mut this.chunk);
                match Pin::new(&mut this.input).poll_read(cx, &mut buf) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Ready(Ok(())) => {
                        this.end = buf.filled().len();
                        this.pos = 0;
                        if this.end == 0 {
                            this.line.clear(); // An unterminated frame is never forwarded.
                            this.eof = true;
                            return Poll::Ready(Ok(()));
                        }
                    }
                }
            }
            let remaining = &this.chunk[this.pos..this.end];
            let newline = remaining.iter().position(|&b| b == b'\n');
            let take = newline.map_or(remaining.len(), |i| i + 1);
            if !this.discarding {
                if this.line.len() + take > MAX_LINE {
                    this.line.clear();
                    this.discarding = true;
                } else {
                    this.line.extend_from_slice(&remaining[..take]);
                }
            }
            this.pos += take;
            if newline.is_some() {
                if this.discarding {
                    this.discarding = false;
                } else if crate::ak820_settings_backup::parse_json(&this.line).is_ok() {
                    this.ready = true;
                } else {
                    // No error details: keys and values may contain secrets.
                    this.line.clear();
                }
            }
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::Arc,
        task::{Wake, Waker},
    };

    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    struct Source {
        data: Vec<u8>,
        pos: usize,
        fragment: usize,
        pending: bool,
        error: bool,
    }
    impl AsyncRead for Source {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            out: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.pending {
                self.pending = false;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            if self.pos == self.data.len() && self.error {
                return Poll::Ready(Err(io::ErrorKind::Other.into()));
            }
            let n = self
                .fragment
                .min(out.remaining())
                .min(self.data.len() - self.pos);
            out.put_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            self.pending = true;
            Poll::Ready(Ok(()))
        }
    }
    fn source(data: &[u8], fragment: usize) -> Source {
        Source {
            data: data.to_vec(),
            pos: 0,
            fragment,
            pending: false,
            error: false,
        }
    }
    fn collect<R: AsyncRead + Unpin>(
        reader: &mut StrictInput<R>,
        size: usize,
    ) -> io::Result<Vec<u8>> {
        let waker = Waker::from(Arc::new(Noop));
        let mut cx = Context::from_waker(&waker);
        let mut result = Vec::new();
        loop {
            let mut bytes = vec![0; size];
            let mut out = ReadBuf::new(&mut bytes);
            match Pin::new(&mut *reader).poll_read(&mut cx, &mut out) {
                Poll::Pending => continue,
                Poll::Ready(Err(e)) => return Err(e),
                Poll::Ready(Ok(())) if out.filled().is_empty() => return Ok(result),
                Poll::Ready(Ok(())) => result.extend_from_slice(out.filled()),
            }
        }
    }
    #[test]
    fn fragmented_frames_and_recovery() {
        let input = "\r\n{\"x\":1,\"x\":2}\n{\"a\":{\"é\":1,\"\\u00e9\":2}}\n{bad}\n{\"v\":\"é\",\"n\":1.5}\r\n[]\npartial".as_bytes();
        let expected = "{\"v\":\"é\",\"n\":1.5}\r\n[]\n".as_bytes();
        for fragment in 1..=input.len() {
            let mut reader = StrictInput::new(source(input, fragment));
            assert_eq!(
                collect(&mut reader, 1).unwrap(),
                expected,
                "fragment {fragment}"
            );
        }
    }
    #[test]
    fn inclusive_limit_and_overlong_resynchronization() {
        let mut valid = vec![b' '; MAX_LINE - 3];
        valid.extend_from_slice(b"{}\n");
        let mut input = valid.clone();
        input.extend_from_slice(&vec![b'x'; MAX_LINE * 3]);
        input.push(b'\n');
        input.extend_from_slice(&vec![b' '; MAX_LINE - 2]);
        input.extend_from_slice(b"{}\n");
        input.extend_from_slice(b"true\n");
        valid.extend_from_slice(b"true\n");
        assert_eq!(
            collect(&mut StrictInput::new(source(&input, 3073)), 7).unwrap(),
            valid
        );
    }
    #[test]
    fn pending_preserves_state_zero_read_and_errors() {
        let mut reader = StrictInput::new(source(b"{\"v\":1}\n", 2));
        let waker = Waker::from(Arc::new(Noop));
        let mut cx = Context::from_waker(&waker);
        let mut zero = [];
        assert!(matches!(
            Pin::new(&mut reader).poll_read(&mut cx, &mut ReadBuf::new(&mut zero)),
            Poll::Ready(Ok(()))
        ));
        let mut buffer = [0; 8];
        assert!(matches!(
            Pin::new(&mut reader).poll_read(&mut cx, &mut ReadBuf::new(&mut buffer)),
            Poll::Pending
        ));
        assert_eq!(collect(&mut reader, 2).unwrap(), b"{\"v\":1}\n");
        let mut failing = source(b"bad", 3);
        failing.error = true;
        assert_eq!(
            collect(&mut StrictInput::new(failing), 4)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Other
        );
    }
    #[test]
    fn invalid_utf8_and_cooperative_budget() {
        let mut input = vec![b'\n'; BUDGET * 2];
        input.extend_from_slice(b"{\"x\":\"\xff\"}\n{}\n");
        let mut reader = StrictInput::new(source(&input, CHUNK));
        let waker = Waker::from(Arc::new(Noop));
        let mut cx = Context::from_waker(&waker);
        let mut buffer = [0; 8];
        assert!(matches!(
            Pin::new(&mut reader).poll_read(&mut cx, &mut ReadBuf::new(&mut buffer)),
            Poll::Pending
        ));
        assert_eq!(collect(&mut reader, 8).unwrap(), b"{}\n");
    }
}
