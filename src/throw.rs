#[macro_export]
macro_rules! throw {
    (return $message:expr) => {{
        log::error!(
            "{} ({}:{})",
            $message,
            file!(),
            line!()
        );
        return Err(anyhow::anyhow!("{}", $message));
    }};
    ($dex:expr, $message:expr) => {{
        log::error!(
            "{} ({}:{})",
            $message,
            file!(),
            line!()
        );
        Err(anyhow::anyhow!("{}", $message))
    }};
}