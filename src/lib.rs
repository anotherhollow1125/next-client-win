#[macro_use]
extern crate anyhow;

pub mod conscon {
    use bindings::Windows::Win32::{Foundation::*, System::Console::*, UI::WindowsAndMessaging::*};
    pub struct ConsoleController {
        con_window: HWND,
    }

    impl ConsoleController {
        pub unsafe fn new() -> Self {
            let con_window = GetConsoleWindow();
            if !con_window.is_null() {
                ShowWindow(con_window, SW_SHOW);
            }

            Self { con_window }
        }
    }

    impl Drop for ConsoleController {
        fn drop(&mut self) {
            unsafe {
                if !self.con_window.is_null() {
                    ShowWindow(self.con_window, SW_HIDE);
                }
            }
        }
    }
}

pub mod config {
    use super::conscon::ConsoleController;
    use anyhow::Result;
    use ini::Ini;
    use log;
    use ncs::errors::NcsError::*;
    use ncs::Command;
    use notify::DebouncedEvent;
    use once_cell::sync::Lazy;
    use regex::Regex;
    use std::ffi::OsStr;
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::str::FromStr;
    use std::sync::mpsc as std_mpsc;
    use std::sync::Mutex;
    use tokio::sync::mpsc as tokio_mpsc;

    pub const CONFFILENAME: &'static str = "conf.ini";

    pub fn conffile_exists() -> bool {
        Path::new(CONFFILENAME).exists()
    }

    pub enum ValidateResult {
        Ok,
        RootPathError,
        DontUseSSLError,
        NetworkError,
    }

    pub struct Config {
        pub nc_host: String,
        pub nc_username: String,
        pub nc_password: String,
        pub local_root: String,
        pub rust_log: log::LevelFilter,
        pub proxy: Option<String>,
    }

    static RE_SSL_CHECK: Lazy<Regex> = Lazy::new(|| Regex::new("^https://.*").unwrap());

    impl Config {
        pub fn load_conf() -> Result<Self> {
            let conf = Ini::load_from_file(CONFFILENAME)?;
            let s = conf.general_section();

            let nc_host = s
                .get("NC_HOST")
                .ok_or_else(|| anyhow!("invalid conf.ini NC_HOST: not found."))?
                .to_string();
            let nc_username = s
                .get("NC_USERNAME")
                .ok_or_else(|| anyhow!("invalid conf.ini NC_USERNAME: not found."))?
                .to_string();
            let nc_password = s
                .get("NC_PASSWORD")
                .ok_or_else(|| anyhow!("invalid conf.ini NC_PASSWORD: not found."))?
                .to_string();
            let local_root = s
                .get("LOCAL_ROOT")
                .ok_or_else(|| anyhow!("invalid conf.ini LOCAL_ROOT: not found."))?
                .to_string();
            let rust_log = s
                .get("RUST_LOG")
                .and_then(|l| log::LevelFilter::from_str(&l).ok())
                .unwrap_or(log::LevelFilter::Off);
            let proxy = s.get("PROXY").map(ToString::to_string);

            Ok(Self {
                nc_host,
                nc_username,
                nc_password,
                local_root,
                rust_log,
                proxy,
            })
        }

        pub fn make_client(&self) -> Result<reqwest::Client> {
            let mut client_builder = reqwest::Client::builder().https_only(true);

            if let Some(proxy_url) = self.proxy.as_ref() {
                let proxy = reqwest::Proxy::https(proxy_url)?;
                client_builder = client_builder.proxy(proxy);
            }

            Ok(client_builder.build()?)
        }

        pub fn save_conf(&self) -> Result<()> {
            let mut conf = Ini::new();
            conf.with_section(None::<String>)
                .set("NC_HOST", &self.nc_host)
                .set("NC_USERNAME", &self.nc_username)
                .set("NC_PASSWORD", &self.nc_password)
                .set("LOCAL_ROOT", &self.local_root)
                .set("RUST_LOG", &self.rust_log.to_string());
            conf.write_to_file(CONFFILENAME)?;

            Ok(())
        }

        pub async fn validation(&self) -> Result<ValidateResult> {
            // root_path check
            let root_path = PathBuf::from(&self.local_root);
            if !root_path.exists() {
                if let Err(_) = std::fs::create_dir_all(&root_path) {
                    return Ok(ValidateResult::RootPathError);
                }
            }

            // ssl check
            if !RE_SSL_CHECK.is_match(&self.nc_host) {
                return Ok(ValidateResult::DontUseSSLError);
            }

            // network check
            let nc_info = ncs::meta::NCInfo::new(
                self.nc_username.clone(),
                self.nc_password.clone(),
                self.nc_host.clone(),
            );
            let url = format!("{}{}", nc_info.host, nc_info.root_path);

            let client = self.make_client()?;
            let res = client
                .request(reqwest::Method::GET, &url)
                .basic_auth(&nc_info.username, Some(&nc_info.password))
                .send()
                .await;

            match res {
                Ok(r) => {
                    if r.status().is_success() {
                        Ok(ValidateResult::Ok)
                    } else {
                        Ok(ValidateResult::NetworkError)
                    }
                }
                Err(_) => Ok(ValidateResult::NetworkError),
            }
        }
    }

    pub unsafe fn prepare_config_file() -> Result<Config> {
        if conffile_exists() {
            return Config::load_conf();
        }

        let _cc = ConsoleController::new();

        let mut nc_host = String::new();
        print!("NC_HOST (ex. https://...): ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut nc_host)?;
        let nc_host = nc_host.trim().to_string();
        let mut nc_username = String::new();
        print!("NC_USERNAME: ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut nc_username)?;
        let nc_username = nc_username.trim().to_string();
        // let mut nc_password = String::new();
        print!("NC_PASSWORD: ");
        io::stdout().flush()?;
        // io::stdin().read_line(&mut nc_password)?;
        let nc_password = rpassword::read_password()?.trim().to_string();
        let mut local_root = String::new();
        print!("watch dir path (ex. c:/Users/...): ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut local_root)?;
        let local_root = local_root.trim().to_string();
        let mut log_level_str = String::new();
        print!("RUST_LOG (default is info): ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut log_level_str)?;
        let rust_log =
            log::LevelFilter::from_str(log_level_str.trim()).unwrap_or(log::LevelFilter::Info);

        let config = Config {
            nc_host,
            nc_username,
            nc_password,
            local_root,
            rust_log,
            proxy: None,
        };

        config.save_conf()?;

        println!("Ok, Configuring Completed.");

        Ok(config)
    }

    pub async fn inifile_watching(
        com_tx: tokio_mpsc::Sender<Command>,
        rx: Mutex<std_mpsc::Receiver<DebouncedEvent>>,
    ) -> Result<()> {
        loop {
            if com_tx.is_closed() {
                return Ok(());
            }

            let c = {
                let rx_ref = rx.lock().map_err(|_| LockError)?;
                match rx_ref.recv() {
                    Ok(DebouncedEvent::Create(p))
                    | Ok(DebouncedEvent::Write(p))
                    | Ok(DebouncedEvent::Remove(p)) => {
                        if p.file_name() == Some(OsStr::new(CONFFILENAME)) {
                            Some(Command::UpdateConfigFile)
                        } else {
                            None
                        }
                    }
                    Ok(_) => None,
                    Err(_e) => {
                        // error!("{:?}", e);
                        return Ok(());
                    }
                }
            };
            if let Some(c) = c {
                com_tx.send(c).await?;
            }
        }
    }
}

pub mod logging {
    use crate::config;
    use anyhow::Result;
    use log4rs::append::console::{ConsoleAppender, Target};
    use log4rs::append::file::FileAppender;
    use log4rs::config::{Appender, Config as log4rsConfig, Root};
    use log4rs::encode::pattern::PatternEncoder;
    use std::path::Path;
    #[allow(unused)]
    use tokio::time::{sleep, Duration};

    pub const TMPLOGFILENAME: &'static str = "tmp.log";

    pub fn prepare_logging_without_logfile(config: &config::Config) -> Result<log4rs::Handle> {
        let log_level = config.rust_log.clone();

        let stderr = ConsoleAppender::builder()
            .encoder(Box::new(PatternEncoder::new(
                "[{d(%Y-%m-%d %H:%M:%S %Z)(utc)} {l} {M}] {m}{n}",
            )))
            .target(Target::Stderr)
            .build();

        if Path::new(TMPLOGFILENAME).exists() {
            fs_extra::remove_items(&[TMPLOGFILENAME])?;
        }

        let tmpfile_appender = FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new(
                "[{d(%Y-%m-%d %H:%M:%S %Z)(utc)} {l} {M}] {m}{n}",
            )))
            .build(TMPLOGFILENAME)?;

        let config = log4rsConfig::builder()
            .appender(Appender::builder().build("stderr", Box::new(stderr)))
            .appender(Appender::builder().build("tmpfile_appender", Box::new(tmpfile_appender)))
            .build(
                Root::builder()
                    .appender("stderr")
                    .appender("tmpfile_appender")
                    .build(log_level),
            )?;

        let handle = log4rs::init_config(config)?;

        Ok(handle)
    }

    pub fn prepare_logging<P>(
        handle: &log4rs::Handle,
        logfile_path: P,
        config: &config::Config,
    ) -> Result<()>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        let log_level = config.rust_log.clone();
        let stderr = ConsoleAppender::builder()
            .encoder(Box::new(PatternEncoder::new(
                "[{d(%Y-%m-%d %H:%M:%S %Z)(utc)} {l} {M}] {m}{n}",
            )))
            .target(Target::Stderr)
            .build();

        let tmpfile_appender = FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new(
                "[{d(%Y-%m-%d %H:%M:%S %Z)(utc)} {l} {M}] {m}{n}",
            )))
            .build(TMPLOGFILENAME)?;

        let file_appender = FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new(
                "[{d(%Y-%m-%d %H:%M:%S %Z)(utc)} {l} {M}] {m}{n}",
            )))
            .build(&logfile_path)?;

        let config = log4rsConfig::builder()
            .appender(Appender::builder().build("stderr", Box::new(stderr)))
            .appender(Appender::builder().build("tmpfile_appender", Box::new(tmpfile_appender)))
            .appender(Appender::builder().build("file_appender", Box::new(file_appender)))
            .build(
                Root::builder()
                    .appender("file_appender")
                    .appender("tmpfile_appender")
                    .appender("stderr")
                    .build(log_level),
            )?;

        handle.set_config(config);

        Ok(())
    }
}

/*
pub mod util {
    pub struct Finally {
        func: Option<Box<dyn FnOnce()>>,
    }

    impl Finally {
        pub fn new(func: Box<dyn FnOnce()>) -> Self {
            Self {
                func: Some(Box::new(func)),
            }
        }
    }

    impl Drop for Finally {
        fn drop(&mut self) {
            if let Some(f) = self.func.take() {
                (f)();
            }
        }
    }
}
*/
