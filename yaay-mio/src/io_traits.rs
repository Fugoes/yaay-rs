use std::future::Future;
use std::io;

use iovec::IoVec;

pub trait AsyncVectoredRead<'a, 'b> {
    type R: Future<Output=io::Result<usize>> + Send + 'a;
    fn read_bufs(&'a self, bufs: &'a mut [&'b mut IoVec]) -> Self::R;
}

pub trait AsyncVectoredWrite<'a, 'b> {
    type R: 'a + Future<Output=io::Result<usize>> + Send;
    fn write_bufs(&'a self, bufs: &'a [&'b IoVec]) -> Self::R;
}
