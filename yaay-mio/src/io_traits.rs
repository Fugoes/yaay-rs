use std::future::Future;
use std::io;

use iovec::IoVec;

pub trait AsyncVectoredRead<'f, 's, 'bufs, 'iovec> {
    type R: 'f + Future<Output=io::Result<usize>> + Send;
    fn read_bufs(&'s self, bufs: &'bufs mut [&'iovec mut IoVec]) -> Self::R;
}

pub trait AsyncVectoredWrite<'f, 's, 'bufs, 'iovec> {
    type R: 'f + Future<Output=io::Result<usize>> + Send;
    fn write_bufs(&'s self, bufs: &'bufs [&'iovec IoVec]) -> Self::R;
}
