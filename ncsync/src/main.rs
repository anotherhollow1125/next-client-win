use bindings::Windows::Win32::{
    Foundation::*, System::DataExchange::COPYDATASTRUCT, UI::WindowsAndMessaging::*,
};

// use anyhow::Result;
use quicli::prelude::*;
use std::ffi::c_void;
use std::mem::size_of;
use structopt::StructOpt;

use ncs::messaging::{NCSyncKind, NCSyncMessage};
use std::convert::Into;
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

#[macro_use]
extern crate if_chain;

#[derive(Debug, StructOpt)]
struct NCSync {
    #[structopt(flatten)]
    verbose: Verbosity,
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    #[structopt(name = "push")]
    /// update remote dir/files by local ones
    Push {
        #[structopt(parse(from_os_str))]
        paths: Vec<PathBuf>,
        #[structopt(short = "r", long = "recursive")]
        /// recursive mode
        recursive: bool,
    },
    #[structopt(name = "pull")]
    /// update local dir/files by remote ones
    Pull {
        #[structopt(parse(from_os_str))]
        paths: Vec<PathBuf>,
        #[structopt(short = "r", long = "recursive")]
        /// recursive mode
        recursive: bool,
        #[structopt(short = "s", long = "stash")]
        /// using stash to save local files
        stash: bool,
    },
    #[structopt(name = "gitzip")]
    GitZip,
    #[structopt(name = "gitunzip")]
    GitUnzip,
}

// MessageCommandが出来てしまった経緯
// もともとimpl Into<Vec<NCSyncMessage>> for Commandだったが、
// GitZipコマンドなどを追加することになり結果的に分けることとなった。
// 冗長なのは認める...直すのめんどい
#[derive(Debug)]
enum MessageCommand {
    Push {
        paths: Vec<PathBuf>,
        recursive: bool,
    },
    Pull {
        paths: Vec<PathBuf>,
        recursive: bool,
        stash: bool,
    },
}

impl Into<Vec<NCSyncMessage>> for MessageCommand {
    fn into(self) -> Vec<NCSyncMessage> {
        let kind: NCSyncKind;
        let pths: Vec<PathBuf>;
        let is_recursive: bool;
        let use_stash: bool;
        match self {
            Self::Push { paths, recursive } => {
                kind = NCSyncKind::Push;
                pths = paths;
                is_recursive = recursive;
                use_stash = false;
            }
            Self::Pull {
                paths,
                recursive,
                stash,
            } => {
                kind = NCSyncKind::Pull;
                pths = paths;
                is_recursive = recursive;
                use_stash = stash;
            }
        }
        let mut messages = Vec::new();
        for path in pths {
            let g = glob::glob(&path.to_string_lossy());
            if let Ok(g) = g {
                for p in g {
                    if_chain! {
                        if let Ok(p) = p;
                        if let Ok(p) = p.canonicalize();
                        then {
                            println!("{:?}", p);
                            let m = NCSyncMessage {
                                kind,
                                is_recursive,
                                use_stash,
                                target: p.to_string_lossy().to_string(),
                            };
                            messages.push(m);
                        }
                    }
                }
            }
        }
        messages
    }
}

fn main() -> CliResult {
    let args = NCSync::from_args();
    args.verbose.setup_env_logger("ncsync")?;

    match args.command {
        Command::Push { paths, recursive } => {
            let m = MessageCommand::Push { paths, recursive };
            unsafe {
                send_messages(m.into())?;
            }
        }
        Command::Pull {
            paths,
            recursive,
            stash,
        } => {
            let m = MessageCommand::Pull {
                paths,
                recursive,
                stash,
            };
            unsafe {
                send_messages(m.into())?;
            }
        }
        Command::GitZip => gitzip()?,
        Command::GitUnzip => gitunzip()?,
    }

    Ok(())
}

unsafe fn send_messages(messages: Vec<NCSyncMessage>) -> CliResult {
    // let target_hwnd = FindWindowA("ncclient", "NextcloudClientWindow");
    let target_hwnd = FindWindowA("ncclient", "NCWindow");

    if target_hwnd.0 == 0 {
        return Err(failure::err_msg("Can't find next-client app!").into());
    }

    println!("{}", target_hwnd.0);

    for message in messages {
        println!("Send Message: {:?}", message);

        let mut v: Vec<u8> = message.into();
        let p = v.as_mut_slice();
        let p_size = p.len() * size_of::<u8>();

        let mut cds = COPYDATASTRUCT {
            dwData: 0,
            cbData: p_size as u32,
            lpData: &mut p[0] as *mut _ as *mut c_void,
        };

        SendMessageA(
            target_hwnd,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM((&mut cds as *mut _) as isize),
        );
    }

    Ok(())
}

fn gitzip() -> CliResult {
    let git_str = "./.git";
    let git_path = Path::new(git_str);
    let gitignore_path = Path::new("./.gitignore");

    if !git_path.exists() {
        return Err(failure::err_msg("There are no git repository").into());
    }

    let curdir = env::current_dir()?;
    let curdir_name = curdir.file_name().unwrap().to_string_lossy();
    let dest = format!("{}_git.zip", curdir_name);

    if !gitignore_path.exists() {
        let _ = fs::File::create(gitignore_path)?;
    }

    let mut already_written = false;
    {
        let f = fs::File::open(gitignore_path)?;
        for line in io::BufReader::new(f).lines() {
            let line = line?;
            if line.contains(&dest) {
                already_written = true;
            }
        }
    }

    if !already_written {
        let mut f = fs::OpenOptions::new().write(true).open(gitignore_path)?;
        writeln!(f, "{}", dest)?;
    }

    match env::consts::OS {
        "windows" => {
            println!("windows");
            std::process::Command::new("powershell.exe")
                .args(["-c", "Compress-Archive"])
                .args(["-Path", git_str])
                .args(["-DestinationPath", &dest])
                .arg("-Force")
                .spawn()?;
        }
        "linux" => {
            println!("linux");
            std::process::Command::new("zip")
                .arg("-r")
                .arg(&dest)
                .arg(git_str)
                .spawn()?;
        }
        _ => (),
    }

    Ok(())
}

fn gitunzip() -> CliResult {
    let target = format!("{}_git.zip", env::current_dir()?.to_string_lossy());

    if !Path::new(&target).exists() {
        return Err(failure::err_msg("target not found.").into());
    }

    match env::consts::OS {
        "windows" => {
            println!("windows");
            std::process::Command::new("powershell.exe")
                .args(["-c", "Expand-Archive"])
                .args(["-Path", &target])
                .args(["-DestinationPath", "."])
                .arg("-Force")
                .spawn()?;
        }
        "linux" => {
            println!("linux");
            std::process::Command::new("unzip").arg(&target).spawn()?;
        }
        _ => (),
    }

    Ok(())
}
