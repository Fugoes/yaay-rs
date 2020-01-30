#[macro_export]
macro_rules! do_io {
    ($mio_box:expr, $io:expr) => {
        loop {
            let waiter = $mio_box.dispatcher.prepare_io();
            match $io {
                Ok(res) => {
                    return Ok(res);
                }
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        waiter.await;
                    } else {
                        return Err(err);
                    };
                }
            };
        };
    };

    ($mio_box:expr, $io:expr, $map:expr) => {
        loop {
            let waiter = $mio_box.dispatcher.prepare_io();
            match $io {
                Ok(res) => {
                    return Ok($map(res));
                }
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        waiter.await;
                    } else {
                        return Err(err);
                    };
                }
            };
        };
    };
}