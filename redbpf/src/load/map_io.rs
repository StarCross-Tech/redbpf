// Copyright 2019 Authors of Red Sift
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use futures::prelude::*;
use mio::unix::EventedFd;
use mio::{Evented, PollOpt, Ready, Token};
use std::io;
use std::os::unix::io::RawFd;
use std::pin::Pin;
use std::slice;
use std::task::{Context, Poll};
use tokio::io::unix::AsyncFd;
use tokio::io::Interest;

use crate::{Event, PerfMap};

// TODO Remove MapIo and upgrade mio.
// It is pub-visibility so removing this is semver breaking change. Since mio
// v0.7, `Evented` is not provided, new version of mio can not be used now.
#[deprecated]
pub struct MapIo(RawFd);

#[allow(deprecated)]
impl Evented for MapIo {
    fn register(
        &self,
        poll: &mio::Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.0).register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &mio::Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.0).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        EventedFd(&self.0).deregister(poll)
    }
}

pub struct PerfMessageStream {
    poll: AsyncFd<RawFd>,
    map: PerfMap,
    name: String,
}

impl PerfMessageStream {
    pub fn new(name: String, map: PerfMap) -> Self {
        let poll = AsyncFd::with_interest(map.fd, Interest::READABLE).unwrap();
        PerfMessageStream { poll, map, name }
    }

    // Note that all messages should be consumed. Because ready flag is
    // cleared, the remaining messages will not be read soon.
    fn read_messages(&mut self) -> Vec<Box<[u8]>> {
        let mut ret = Vec::new();
        while let Some(ev) = self.map.read() {
            match ev {
                Event::Lost(lost) => {
                    eprintln!("Possibly lost {} samples for {}", lost.count, &self.name);
                }
                Event::Sample(sample) => {
                    let msg = unsafe {
                        slice::from_raw_parts(sample.data.as_ptr(), sample.size as usize)
                            .to_vec()
                            .into_boxed_slice()
                    };
                    ret.push(msg);
                }
            };
        }

        ret
    }
}

impl Stream for PerfMessageStream {
    type Item = Vec<Box<[u8]>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.poll.poll_read_ready(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => {
                // it should never happen
                eprintln!("PerfMessageStream error: {:?}", e);
                return Poll::Ready(None);
            }
            Poll::Ready(Ok(mut rg)) => rg.clear_ready(),
        };
        // Must read all messages because AsyncFdReadyGuard::clear_ready is
        // already called.
        Some(self.read_messages()).into()
    }
}
