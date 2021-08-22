use anyhow::Result;
use bindings::Windows::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleA,
    UI::{Shell::*, WindowsAndMessaging::*},
};
use dotenv::dotenv;
use log::{debug, error, info};
use ncs::errors::NcsError::*;
use ncs::local_listen::*;
use ncs::meta::*;
use ncs::nc_listen::*;
use ncs::network::{self, NetworkStatus};
use ncs::*;
use notify::{watcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::Path;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;
use tokio::sync::mpsc as tokio_mpsc;
#[allow(unused)]
use tokio::time::{sleep, Duration};
#[macro_use]
extern crate if_chain;

macro_rules! terminate_send {
    ($tx:expr) => {
        let mut counter: u32 = 0;
        while let Err(e) = $tx.send(Command::Terminate).await {
            info!("{:?}", e);
            counter += 1;
            if counter > 3 {
                break;
            }
        }
    };
}

async fn run(tray_rx: Arc<Mutex<std_mpsc::Receiver<TasktrayMessage>>>) -> Result<bool> {
    // Can't update these environment variables by rerun.
    let username = env::var("NC_USERNAME").expect("NC_USERNAME not found");
    let password = env::var("NC_PASSWORD").expect("NC_PASSWORD not found");
    let host = env::var("NC_HOST").expect("NC_HOST not found");
    let host = fix_host(&host);

    let nc_info = NCInfo::new(username, password, host);

    let local_root_path = env::var("LOCAL_ROOT").expect("LOCAL_ROOT not found");
    let local_info = LocalInfo::new(local_root_path)?;

    let public_resource: PublicResource;
    if Path::new(local_info.get_cachefile_name().as_str()).exists() {
        // load cache
        let ncs_cache = load_cache(&local_info)?;
        let nc_state = NCState {
            latest_activity_id: ncs_cache.latest_activity_id,
        };
        let root_entry = json_entry2entry(ncs_cache.root_entry)?;
        public_resource = PublicResource::new(root_entry, nc_state);
    } else {
        // init
        if !network::is_online(&nc_info).await {
            return Err(NetworkOfflineError.into());
        }

        let (root, latest_activity_id) = init(&nc_info, &local_info).await?;
        let json_entry = {
            let root_ref = root.lock().map_err(|_| LockError)?;
            root2json_entry(&root_ref)?
        };
        save_cache(latest_activity_id.clone(), json_entry, &local_info)?;
        let nc_state = NCState {
            latest_activity_id: latest_activity_id,
        };
        public_resource = PublicResource::new(root, nc_state);
    }

    let public_resource = Arc::new(Mutex::new(public_resource));

    // to end with successful completion, watchers must be managed here.

    let (tx, rx) = std_mpsc::channel();
    let mut root_watcher = watcher(tx, StdDuration::from_secs(5)).unwrap();
    root_watcher.watch(&local_info.root_path, RecursiveMode::Recursive)?;
    let loceve_rx = Mutex::new(rx);

    let (tx, rx) = std_mpsc::channel();
    let mut meta_watcher = watcher(tx, StdDuration::from_secs(5)).unwrap();
    meta_watcher.watch(
        local_info.get_metadir_name().as_str(),
        RecursiveMode::Recursive,
    )?;
    let metaeve_rx = Mutex::new(rx);

    let (com_tx, mut com_rx) = tokio_mpsc::channel(32);

    let tx = com_tx.clone();
    let lci = local_info.clone();
    let nci = nc_info.clone();
    let watching_handle = tokio::spawn(async move {
        let res = watching(tx.clone(), loceve_rx, &lci, &nci).await;
        if let Err(e) = res {
            info!("{:?}", e);
            terminate_send!(tx);
        }
    });

    let tx = com_tx.clone();
    let lci = local_info.clone();
    let updateexcfile_handle = tokio::spawn(async move {
        let res = exc_list_update_watching(tx.clone(), metaeve_rx, &lci).await;
        if let Err(e) = res {
            info!("{:?}", e);
            terminate_send!(tx);
        }
    });

    let nc_state = {
        let pr_ref = public_resource.lock().map_err(|_| LockError)?;
        pr_ref.nc_state.clone()
    };
    let tx = com_tx.clone();
    let nci = nc_info.clone();
    let lci = local_info.clone();
    let nclisten_handle = tokio::spawn(async move {
        let res = nclistening(tx.clone(), &nci, &lci, nc_state.clone()).await;
        if let Err(e) = res {
            info!("{:?}", e);
            terminate_send!(tx);
        }
    });

    let tx = com_tx.clone();
    let control_handle = tokio::spawn(async move {
        loop {
            let r = {
                let res = tray_rx.lock();
                match res {
                    Ok(rx) => Some(rx.recv()),
                    Err(_) => None,
                }
            };

            match r {
                Some(Ok(m)) => {
                    let com = match m {
                        TasktrayMessage::Repair => Command::NormalRepair,
                        TasktrayMessage::Exit => Command::Terminate,
                        _ => continue,
                    };
                    let res = tx.send(com).await;
                    if let Err(e) = res {
                        error!("{:?}", e);
                        terminate_send!(tx);
                        return;
                    }
                }
                Some(Err(e)) => {
                    error!("{:?}", e);
                    terminate_send!(tx);
                    return;
                }
                _ => {
                    error!("Something wrong.");
                    terminate_send!(tx);
                    return;
                }
            }
            break;
        }
    });

    let mut network_status = network::status(&nc_info).await?;
    let mut nc2l_cancel_map = HashMap::new();
    let mut l2nc_cancel_set = HashSet::new();
    let mut offline_locevent_que: Vec<local_listen::LocalEvent> = Vec::new();
    let mut retry = false;
    while let Some(e) = com_rx.recv().await {
        match e {
            Command::LocEvent(ev) => match network_status {
                NetworkStatus::Connect => {
                    let pr_ref = public_resource.lock().map_err(|_| LockError)?;
                    let res = deal_local_event(
                        ev,
                        &pr_ref.root,
                        &nc_info,
                        &local_info,
                        &mut nc2l_cancel_map,
                        &mut l2nc_cancel_set,
                    )
                    .await;
                    if let Err(e) = res {
                        info!("{:?}", e);
                        // break;
                    }
                }
                NetworkStatus::Disconnect | NetworkStatus::Err(_) => {
                    debug!("LocEvent({:?}) @ offline", ev);
                    offline_locevent_que.push(ev);
                }
            },
            Command::NCEvents(ev_vec, new_state) => match network_status {
                NetworkStatus::Connect => {
                    debug!("NCEvents({:?})", new_state);
                    let mut pr_ref = public_resource.lock().map_err(|_| LockError)?;

                    if pr_ref.nc_state.eq_or_newer_than(&new_state) {
                        continue;
                    }

                    pr_ref.nc_state = new_state;
                    let res = update_and_download(
                        ev_vec,
                        &pr_ref.root,
                        &nc_info,
                        &local_info,
                        &mut nc2l_cancel_map,
                        &mut l2nc_cancel_set,
                    )
                    .await;
                    if let Err(e) = res {
                        info!("{:?}", e);
                        // break;
                    }
                }
                NetworkStatus::Disconnect | NetworkStatus::Err(_) => {
                    info!("It should be unreachable branch. something wrong.");
                }
            },
            Command::UpdateExcFile => {
                retry = true;
                break;
            }
            Command::HardRepair => {
                drop(root_watcher);
                drop(meta_watcher);
                com_rx.close();
                nclisten_handle.await?;
                watching_handle.await?;
                updateexcfile_handle.await?;
                control_handle.abort();
                repair::all_delete(&local_info)?;
                return Ok(true);
            }
            Command::NormalRepair => {
                let events = {
                    let mut pr_ref = public_resource.lock().map_err(|_| LockError)?;
                    get_ncevents(&nc_info, &local_info, &mut pr_ref.nc_state).await?
                };
                repair::normal_repair(&local_info, &nc_info, &public_resource, events).await?;
                sleep(Duration::from_secs(20)).await;
                retry = true;
                break;
            }
            Command::NetworkConnect => match network_status {
                NetworkStatus::Connect => (),
                _ => {
                    // Reconnect situation
                    let res = repair::soft_repair(
                        &local_info,
                        &nc_info,
                        &public_resource,
                        offline_locevent_que.drain(..).collect(),
                        com_tx.clone(),
                        &mut nc2l_cancel_map,
                        &mut l2nc_cancel_set,
                    )
                    .await?;
                    if res {
                        retry = true;
                        break;
                    } else {
                        network_status = NetworkStatus::Connect;
                        retry = false;
                    }
                }
            },
            Command::NetworkDisconnect => match network_status {
                NetworkStatus::Connect => {
                    // disconnect situation
                    nc2l_cancel_map = HashMap::new();
                    l2nc_cancel_set = HashSet::new();
                    network_status = NetworkStatus::Disconnect;
                }
                _ => (),
            },
            Command::Terminate => break,
        }
    }

    drop(root_watcher);
    drop(meta_watcher);

    com_rx.close();

    nclisten_handle.await?;
    watching_handle.await?;
    updateexcfile_handle.await?;
    control_handle.abort();

    let pr_ref = public_resource.lock().map_err(|_| LockError)?;
    let json_entry = {
        let r = pr_ref.root.lock().map_err(|_| LockError)?;
        debug!("\n{}", r.get_tree());
        root2json_entry(&r)?
    };
    save_cache(
        pr_ref.nc_state.latest_activity_id.clone(),
        json_entry,
        &local_info,
    )?;

    Ok(retry)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    env_logger::init();

    let (tray_tx, tray_rx) = std_mpsc::channel();

    unsafe {
        P_TX = Some(tray_tx);

        let tasktray_handle = std::thread::spawn(|| {
            let res = tasktray();
            if let Err(e) = res {
                error!("{:?}", e);
            }
        });

        let tray_rx = Arc::new(Mutex::new(tray_rx));
        while run(tray_rx.clone()).await? {}
        drop(tray_rx);

        let _ = tasktray_handle.join();
    }

    Ok(())
}

async fn init(nc_info: &NCInfo, local_info: &LocalInfo) -> Result<(ArcEntry, String)> {
    let root_entry = from_nc_all(nc_info, local_info, "/").await?;
    let latest_activity_id = get_latest_activity_id(nc_info).await?;
    debug!("{}", latest_activity_id);

    init_local_entries(nc_info, local_info, &root_entry, "").await?;

    {
        let r = root_entry.lock().map_err(|_| LockError)?;
        println!("\n{}", r.get_tree());
    }

    Ok((root_entry, latest_activity_id))
}

#[macro_use]
extern crate anyhow;
use std::{mem, ptr};

const MYMSG_TRAY: u32 = WM_APP + 1;
const ID_MYTRAY: u32 = 56562; // ゴロゴロニャーちゃん

const MSGID_REPAIR: u32 = 40001;
const MSGID_EXIT: u32 = 40002;

static mut P_NID: *mut NOTIFYICONDATAW = ptr::null_mut();
static mut P_HMENU: *mut HMENU = ptr::null_mut();
static mut P_TX: Option<std_mpsc::Sender<TasktrayMessage>> = None;

enum TasktrayMessage {
    Nop,
    Repair,
    Exit,
}

unsafe fn tasktray() -> Result<()> {
    let instance = GetModuleHandleA(None);

    if instance.0 == 0 {
        return Err(anyhow!("instance.0 == 0"));
    }

    let window_class = "window";

    let wc = WNDCLASSA {
        hCursor: LoadCursorW(None, IDC_ARROW),
        hInstance: instance,
        lpszClassName: PSTR(b"window\0".as_ptr() as _),

        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        ..Default::default()
    };

    let atom = RegisterClassA(&wc);

    if atom == 0 {
        return Err(anyhow!("atom == 0"));
    }

    let hwnd = CreateWindowExA(
        Default::default(),
        window_class,
        "This is a sample window",
        WS_OVERLAPPEDWINDOW,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        0,
        0,
        None,
        None,
        instance,
        std::ptr::null_mut(),
    );

    let mut nid = create_nid(hwnd);
    P_NID = &mut nid;

    let tmp = LoadMenuW(instance, "NC_CONTROL");
    P_HMENU = &mut GetSubMenu(tmp, 0);

    Shell_NotifyIconW(NIM_ADD, P_NID);
    ShowWindow(hwnd, SW_HIDE);

    let mut message = MSG::default();

    while GetMessageA(&mut message, HWND(0), 0, 0).into() {
        DispatchMessageA(&mut message);

        if_chain! {
            if let Some(tx) = P_TX.as_ref();
            if let Err(_) = tx.send(TasktrayMessage::Nop);
            then {
                break;
            }
        }
    }

    Ok(())
}

fn encode(source: &str) -> Vec<u16> {
    source.encode_utf16().chain(Some(0)).collect()
}

unsafe fn create_nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid = mem::zeroed::<NOTIFYICONDATAW>();
    nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.uID = ID_MYTRAY;
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.hWnd = hwnd;
    nid.uCallbackMessage = MYMSG_TRAY;
    nid.hIcon = LoadIconW(None, IDI_APPLICATION);
    let mut buf = [0u16; 128];
    let tip = "Hello, World!";
    ptr::copy(encode(tip).as_ptr(), &mut buf[0], tip.len());
    nid.szTip = buf;
    nid
}

extern "system" fn wndproc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message as u32 {
            MYMSG_TRAY => {
                match (wparam.0 as u32, lparam.0 as u32) {
                    (ID_MYTRAY, WM_RBUTTONUP) | (ID_MYTRAY, WM_LBUTTONUP) => {
                        let mut p = POINT { x: 0, y: 0 };
                        GetCursorPos(&mut p);
                        // ClientToScreen(window, &mut p);
                        TrackPopupMenu(
                            *P_HMENU,
                            TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                            p.x,
                            p.y,
                            0,
                            window,
                            ptr::null_mut(),
                        );
                    }
                    _ => return DefWindowProcA(window, message, wparam, lparam),
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                if let Some(tx) = P_TX.as_ref() {
                    tx.send(TasktrayMessage::Exit).ok();
                }
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_COMMAND => {
                match (wparam.0 as u32, lparam.0 as u32) {
                    (MSGID_REPAIR, _) => {
                        debug!("TASKTRAY REPAIR");
                        if let Some(tx) = P_TX.as_ref() {
                            tx.send(TasktrayMessage::Repair).ok();
                        }
                    }
                    (MSGID_EXIT, _) => {
                        debug!("TASKTRAY EXIT");
                        if let Some(tx) = P_TX.as_ref() {
                            tx.send(TasktrayMessage::Exit).ok();
                            PostQuitMessage(0);
                        }
                    }
                    _ => return DefWindowProcA(window, message, wparam, lparam),
                }
                LRESULT(0)
            }
            _ => DefWindowProcA(window, message, wparam, lparam),
        }
    }
}
