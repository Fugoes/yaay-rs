#[macro_export]
macro_rules! io_poll {
    ($dispatcher:expr, $io:expr, $map:expr) => {
        loop {
            $dispatcher.prepare_io();
            match $io {
                Ok(res) => {
                    return Ok($map(res));
                }
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        $dispatcher.wait_io().await;
                    } else {
                        return Err(err);
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
                    return Ok(res);
                }
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        $dispatcher.wait_io().await;
                    } else {
                        return Err(err);
                    };
                }
            };
        };
    };
}
