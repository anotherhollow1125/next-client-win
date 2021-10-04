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
use std::path::PathBuf;

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
    /// update remote dir/files by local ones.
    Push {
        #[structopt(parse(from_os_str))]
        paths: Vec<PathBuf>,
        #[structopt(short = "r", long = "recursive")]
        /// recursive mode
        recursive: bool,
    },
    #[structopt(name = "pull")]
    /// update local dir/files by remote ones.
    Pull {
        #[structopt(parse(from_os_str))]
        paths: Vec<PathBuf>,
        #[structopt(short = "r", long = "recursive")]
        /// recursive mode
        recursive: bool,
    },
}

impl Into<Vec<NCSyncMessage>> for Command {
    fn into(self) -> Vec<NCSyncMessage> {
        let kind: NCSyncKind;
        let pths: Vec<PathBuf>;
        let is_recursive: bool;
        match self {
            Self::Push { paths, recursive } => {
                kind = NCSyncKind::Push;
                pths = paths;
                is_recursive = recursive;
            }
            Self::Pull { paths, recursive } => {
                kind = NCSyncKind::Pull;
                pths = paths;
                is_recursive = recursive;
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

    let messages: Vec<NCSyncMessage> = args.command.into();

    unsafe {
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
    }

    Ok(())
}
