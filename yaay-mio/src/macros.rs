#[macro_export]
macro_rules! io_poll {
    ($dispatcher:expr, $io:expr, $map:expr) => {
        loop {
            $dispatcher.prepare_io();
            match $io {
                Ok(res) => {
                    return std::task::Poll::Ready(Ok($map(res)));
                }
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        if $dispatcher.try_wait_io() {
                            return std::task::Poll::Pending;
                        };
                    } else {
                        return std::task::Poll::Ready(Err(err));
                    };
                }
            };
        };
    };

    ($dispatcher:expr, $io:expr) => {
        loop {
            $dispatcher.prepare_io();
            match $io {
                Ok(res) => {
                    return std::task::Poll::Ready(Ok(res));
                }
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        if $dispatcher.try_wait_io() {
                            return std::task::Poll::Pending;
                        };
                    } else {
                        return std::task::Poll::Ready(Err(err));
                    };
                }
            };
        };
    };
}
