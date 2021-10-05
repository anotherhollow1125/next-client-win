// #![cfg_attr(debug_assertions, windows_subsystem = "windows")]
use anyhow::Result;
use bindings::Windows::Win32::{
    Foundation::*,
    System::{Console::*, DataExchange::COPYDATASTRUCT, LibraryLoader::GetModuleHandleA},
    UI::{Shell::*, WindowsAndMessaging::*},
};
use log::{debug, error, info, warn};
use ncs::errors::NcsError::*;
use ncs::local_listen::*;
use ncs::meta::*;
use ncs::nc_listen::*;
use ncs::network::{self, NetworkStatus};
use ncs::*;
use next_client_win::{config, logging, ncsync_daemon};
use notify::{watcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;
use tokio::sync::mpsc as tokio_mpsc;
#[allow(unused)]
use tokio::time::{sleep, Duration};
#[macro_use]
extern crate if_chain;

macro_rules! error_send {
    ($tx:expr, $e:expr) => {
        if let Err(re) = $tx.send(Command::Error($e)).await {
            warn!("{:?} : Receiver dropped.", re);
        }
    };
}

async fn run(
    tray_tx: std_mpsc::Sender<TasktrayMessage>,
    tray_rx: Arc<Mutex<std_mpsc::Receiver<TasktrayMessage>>>,
    ncsyncmes_tx: std_mpsc::Sender<Option<NCSyncMessage>>,
    ncsyncmes_rx: Arc<Mutex<std_mpsc::Receiver<Option<NCSyncMessage>>>>,
    icon_tx: std_mpsc::Sender<IconChange>,
    log_handle: &log4rs::Handle,
    _debug_counter: u32,
    config: &config::Config,
) -> Result<bool> {
    icon_tx.send(IconChange::Load).ok();

    match config.validation().await? {
        config::ValidateResult::Ok => (),
        config::ValidateResult::RootPathError => {
            return Err(anyhow!("[config error] Please fix ROOT_PATH in conf.ini"));
        }
        config::ValidateResult::DontUseSSLError => {
            return Err(anyhow!(
                "[config error] NC Host must use SSL/TLS. Please set \"https://...\" to NC_HOST."
            ));
        }
        config::ValidateResult::NetworkError => {
            warn!(
                "[config error] host, username or password are wrong or Network is disconnect.
Please fix conf.ini and connect to the Internet."
            );
        }
    }

    // Can't update these environment variables by rerun.
    let username = config.nc_username.clone();
    let password = config.nc_password.clone();
    let host = config.nc_host.clone();
    let host = fix_host(&host);

    let nc_info = NCInfo::new(username, password, host);

    let local_root_path = config.local_root.clone();
    let client = config.make_client()?;
    let local_info = LocalInfo::new(local_root_path, client.clone())?;

    let logfile_path = local_info.get_logfile_name();
    logging::prepare_logging(log_handle, logfile_path, config)?;

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
        if !network::is_online(&nc_info, &client).await {
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

    let (tx, rx) = std_mpsc::channel();
    let mut ini_watcher = watcher(tx, StdDuration::from_secs(5)).unwrap();
    ini_watcher.watch(".", RecursiveMode::NonRecursive)?;
    let ini_rx = Mutex::new(rx);

    let (com_tx, mut com_rx) = tokio_mpsc::channel(32);

    let tx = com_tx.clone();
    let lci = local_info.clone();
    let nci = nc_info.clone();
    let _watching_handle = tokio::spawn(async move {
        let res = watching(tx.clone(), loceve_rx, &lci, &nci).await;
        if let Err(e) = res {
            error_send!(tx, e);
        }
    });

    let tx = com_tx.clone();
    let lci = local_info.clone();
    let _updateexcfile_handle = tokio::spawn(async move {
        let res = exc_list_update_watching(tx.clone(), metaeve_rx, &lci).await;
        if let Err(e) = res {
            error_send!(tx, e);
        }
    });

    let tx = com_tx.clone();
    let _updateconfigfile_handle = tokio::spawn(async move {
        let res = config::inifile_watching(tx.clone(), ini_rx).await;
        if let Err(e) = res {
            error_send!(tx, e);
        }
    });

    let nc_state = {
        let pr_ref = public_resource.lock().map_err(|_| LockError)?;
        pr_ref.nc_state.clone()
    };
    let tx = com_tx.clone();
    let nci = nc_info.clone();
    let lci = local_info.clone();
    let _nclisten_handle = tokio::spawn(async move {
        let res = nclistening(tx.clone(), &nci, &lci, nc_state.clone()).await;
        if let Err(e) = res {
            error_send!(tx, e);
        }
    });

    let tx = com_tx.clone();
    let lci = local_info.clone();
    let _control_handle = tokio::spawn(async move {
        // このループはExcEditとNop以外はcontinueしないので落ちます！！
        loop {
            let r = {
                let res = tray_rx.lock();
                match res {
                    Ok(rx) => Some(rx.recv()),
                    Err(_) => None,
                }
            };

            if tx.is_closed() {
                break;
            }

            match r {
                Some(Ok(m)) => {
                    // info!("control_handle: {:?}", m);
                    let com = match m {
                        TasktrayMessage::Repair => Command::NormalRepair,
                        TasktrayMessage::Restart => Command::Terminate(true),
                        TasktrayMessage::Exit => Command::Terminate(false),
                        TasktrayMessage::ExcEdit => {
                            let exc = lci.get_excludefile_name();
                            open_notepad(exc);
                            continue;
                        }
                        TasktrayMessage::Nop => continue,
                    };
                    let res = tx.send(com).await;
                    if let Err(e) = res {
                        error!("! {:?}", e);
                        return;
                    }
                }
                Some(Err(e)) => {
                    error_send!(tx, e.into());
                    return;
                }
                _ => {
                    error_send!(tx, anyhow!("Something wrong"));
                    return;
                }
            }
            // because Repair, Restart, Exit command is not repeat this control.
            break;
        }
    });

    let tx = com_tx.clone();
    let lci = local_info.clone();
    let _ncsyncmes_handle = tokio::spawn(async move {
        loop {
            let r = {
                let res = ncsyncmes_rx.lock();
                match res {
                    Ok(rx) => Some(rx.recv()),
                    Err(_) => None,
                }
            };

            if tx.is_closed() {
                break;
            }

            match r {
                Some(Ok(Some(m))) => {
                    // debug!("catch: {:?}", m);
                    let res = ncsync_daemon::forge_event(m, &tx, &lci).await;
                    if let Err(e) = res {
                        error!("NCSM {:?}", e);
                        error_send!(tx, e.into());
                        return;
                    }
                }
                Some(Ok(None)) => break,
                Some(Err(e)) => {
                    error_send!(tx, e.into());
                    return;
                }
                _ => {
                    error_send!(tx, anyhow!("Something wrong"));
                    return;
                }
            }
        }
    });

    let mut network_status = network::status(&nc_info, &client).await?;
    let mut nc2l_cancel_map = HashMap::new();
    let mut l2nc_cancel_set = HashSet::new();
    let mut offline_locevent_que: Vec<local_listen::LocalEvent> = Vec::new();
    let mut retry = Ok(false);
    let mut current_icon = IconChange::Normal;
    icon_tx.send(IconChange::Normal).ok();
    info!("Main Loop Start");
    while let Some(e) = com_rx.recv().await {
        match e {
            Command::LocEvent(ev) => match network_status {
                NetworkStatus::Connect => {
                    icon_tx.send(IconChange::Load).ok();
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
                        error!("L {:?}", e);
                        icon_tx.send(IconChange::Error).ok();
                        current_icon = IconChange::Error;
                        continue;
                    }
                    icon_tx.send(current_icon).ok();
                }
                NetworkStatus::Disconnect | NetworkStatus::Err(_) => {
                    info!("LocEvent({:?}) @ offline", ev);
                    offline_locevent_que.push(ev);
                }
            },
            Command::NCEvents(ev_vec, new_state) => match network_status {
                NetworkStatus::Connect => {
                    icon_tx.send(IconChange::Load).ok();
                    info!("NCEvents({:?})", new_state);
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
                        false,
                    )
                    .await;
                    if let Err(e) = res {
                        error!("NC {:?}", e);
                        icon_tx.send(IconChange::Error).ok();
                        current_icon = IconChange::Error;
                        continue;
                    }
                    icon_tx.send(current_icon).ok();
                }
                NetworkStatus::Disconnect | NetworkStatus::Err(_) => {
                    error!("It should be unreachable branch. something wrong.");
                }
            },
            Command::PullEvent {
                target,
                is_recursive,
                stash,
            } => {
                icon_tx.send(IconChange::Load).ok();
                info!(
                    "PullEvent({:?}, -r: {:?}, -s: {:?})",
                    target, is_recursive, stash
                );
                let pr_ref = public_resource.lock().map_err(|_| LockError)?;

                let res = nc_listen::refresh(
                    target,
                    is_recursive,
                    &pr_ref.root,
                    &nc_info,
                    &local_info,
                    &mut nc2l_cancel_map,
                    stash,
                )
                .await;

                if let Err(e) = res {
                    error!("PULL {:?}", e);
                    // icon_tx.send(IconChange::Error).ok();
                    // current_icon = IconChange::Error;
                    continue;
                }
                icon_tx.send(current_icon).ok();
            }
            Command::UpdateExcFile => {
                icon_tx.send(IconChange::Load).ok();
                info!("Update Exclude targets file.");
                info!("Rebooting...");
                retry = Ok(true);
                break;
            }
            Command::UpdateConfigFile => {
                icon_tx.send(IconChange::Load).ok();
                info!("Update Config file.");
                info!("Rebooting...");
                retry = Ok(true);
                break;
            }
            Command::HardRepair => {
                icon_tx.send(IconChange::Load).ok();
                info!("Hard Repair Start.");
                drop(root_watcher);
                drop(meta_watcher);
                com_rx.close();

                /*
                nclisten_handle.abort();
                watching_handle.abort();
                updateexcfile_handle.abort();
                updateconfigfile_handle.abort();
                control_handle.abort();
                */
                repair::all_delete(&local_info)?;
                info!("Rebooting...");
                return Ok(true);
            }
            Command::NormalRepair => {
                icon_tx.send(IconChange::Load).ok();
                info!("Normal Repair Start");
                let events = {
                    let mut pr_ref = public_resource.lock().map_err(|_| LockError)?;
                    get_ncevents(&nc_info, &local_info, &mut pr_ref.nc_state).await?
                };
                repair::normal_repair(&local_info, &nc_info, &public_resource, events).await?;
                sleep(Duration::from_secs(20)).await;
                info!("Rebooting...");
                retry = Ok(true);
                break;
            }
            Command::NetworkConnect => match network_status {
                NetworkStatus::Connect => (),
                _ => {
                    info!("Network Connection Restored.");
                    icon_tx.send(IconChange::Load).ok();
                    // Reconnect situation
                    let have_to_rerun = repair::soft_repair(
                        &local_info,
                        &nc_info,
                        &public_resource,
                        offline_locevent_que.drain(..).collect(),
                        com_tx.clone(),
                        &mut nc2l_cancel_map,
                        &mut l2nc_cancel_set,
                    )
                    .await?;
                    if have_to_rerun {
                        icon_tx.send(IconChange::Load).ok();
                        retry = Ok(true);
                        break;
                    } else {
                        network_status = NetworkStatus::Connect;
                        icon_tx.send(IconChange::Normal).ok();
                        retry = Ok(false);
                    }
                }
            },
            Command::NetworkDisconnect => match network_status {
                NetworkStatus::Connect => {
                    info!("Lost Network Connection.");
                    icon_tx.send(IconChange::Offline).ok();
                    // disconnect situation
                    nc2l_cancel_map = HashMap::new();
                    l2nc_cancel_set = HashSet::new();
                    network_status = NetworkStatus::Disconnect;
                }
                _ => (),
            },
            Command::Terminate(r) => {
                icon_tx.send(IconChange::Load).ok();
                retry = Ok(r);
                break;
            }
            Command::Error(e) => {
                retry = Err(e);
                break;
            }
        }
    }

    drop(root_watcher);
    drop(meta_watcher);

    // to close tasktray handle.
    let _ = tray_tx.send(TasktrayMessage::Nop);
    // to close ncsyncmes handle.
    let _ = ncsyncmes_tx.send(None);

    com_rx.close();

    /*
    nclisten_handle.abort();
    watching_handle.abort();
    updateexcfile_handle.abort();
    updateconfigfile_handle.abort();
    control_handle.abort();
    */

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

    debug!("run func end.");

    retry
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut config = unsafe { config::prepare_config_file()? };
    let log_handle = logging::prepare_logging_without_logfile(&config)?;

    let (tray_tx, tray_rx) = std_mpsc::channel();
    let (emergency_tx, emergency_rx) = std_mpsc::channel();
    let (ncsyncmes_tx, ncsyncmes_rx) = std_mpsc::channel();

    unsafe {
        P_TX = Some(tray_tx.clone());
        P_EMERGENCY_TX = Some(emergency_tx.clone());
        P_NCSYNCMES_TX = Some(ncsyncmes_tx.clone());

        let (icon_tx, icon_rx) = std_mpsc::channel();
        let tasktray_handle = std::thread::spawn(|| {
            let res = tasktray(icon_rx);
            if let Err(e) = res {
                error!("{:?}", e);
            }
        });

        let tray_rx = Arc::new(Mutex::new(tray_rx));
        let ncsyncmes_rx = Arc::new(Mutex::new(ncsyncmes_rx));
        let mut debug_counter = 1;
        loop {
            let cntn_w = run(
                tray_tx.clone(),
                tray_rx.clone(),
                ncsyncmes_tx.clone(),
                ncsyncmes_rx.clone(),
                icon_tx.clone(),
                &log_handle,
                debug_counter,
                &config,
            )
            .await;

            match cntn_w {
                Ok(true) => (),
                Ok(false) => break,
                Err(e) => {
                    error!("# {}", e);
                    icon_tx.send(IconChange::Error).ok();
                    match emergency_rx.recv() {
                        Ok(false) | Err(_) => break,
                        _ => {
                            if let Ok(rx) = tray_rx.lock() {
                                // to consume TasktrayMessage::Restart
                                while let Ok(v) = rx.try_recv() {
                                    debug!("v: {:?}", v);
                                }
                                debug!("while let break.");
                            }
                        }
                    }
                }
            }

            config = config::prepare_config_file()?;
            debug_counter += 1;
        }
        drop(tray_rx);
        drop(emergency_rx);

        icon_tx.send(IconChange::Terminate).ok();

        let _ = tasktray_handle.join();
    }

    Ok(())
}

async fn init(nc_info: &NCInfo, local_info: &LocalInfo) -> Result<(ArcEntry, String)> {
    let root_entry = from_nc_all(nc_info, local_info, "/").await?;
    let latest_activity_id = get_latest_activity_id(nc_info, local_info).await?;
    debug!("{}", latest_activity_id);

    init_local_entries(nc_info, local_info, &root_entry, "").await?;

    {
        let r = root_entry.lock().map_err(|_| LockError)?;
        debug!("\n{}", r.get_tree());
    }

    Ok((root_entry, latest_activity_id))
}

#[macro_use]
extern crate anyhow;
use ncs::messaging::*;
use std::{convert::TryFrom, slice::from_raw_parts};
use std::{mem, ptr};

const MYMSG_TRAY: u32 = WM_APP + 1;
const ID_MYTRAY: u32 = 56562; // ゴロゴロニャーちゃん

const MSGID_SHOWLOG: u32 = 40001;
const MSGID_EDITCONF: u32 = 40002;
const MSGID_EDITEXCLUDE: u32 = 40003;
const MSGID_REPAIR: u32 = 40004;
const MSGID_RESTART: u32 = 40009;
const MSGID_EXIT: u32 = 40010;

static mut P_NID: *mut NOTIFYICONDATAW = ptr::null_mut();
static mut P_NID_NORMAL: *mut NOTIFYICONDATAW = ptr::null_mut();
static mut P_NID_LOAD: *mut NOTIFYICONDATAW = ptr::null_mut();
static mut P_NID_ERROR: *mut NOTIFYICONDATAW = ptr::null_mut();
static mut P_NID_OFFLINE: *mut NOTIFYICONDATAW = ptr::null_mut();
static mut P_HMENU: *mut HMENU = ptr::null_mut();
static mut P_TX: Option<std_mpsc::Sender<TasktrayMessage>> = None;
static mut P_EMERGENCY_TX: Option<std_mpsc::Sender<bool>> = None;
static mut P_NCSYNCMES_TX: Option<std_mpsc::Sender<Option<NCSyncMessage>>> = None;

#[derive(Debug)]
enum TasktrayMessage {
    Nop,
    ExcEdit,
    Repair,
    Restart,
    Exit,
}

#[derive(Debug, Clone, Copy)]
enum IconChange {
    Normal,
    Load,
    Error,
    Offline,
    Terminate,
}

/*
fn encodeu8(source: &str) -> Vec<u8> {
    let t = source.as_bytes().into_iter();
    let _ = t.chain(vec![&0u8]);

    vec![]
    // .chain(vec![0]).collect()
}
*/

unsafe fn tasktray(icon_rx: std_mpsc::Receiver<IconChange>) -> Result<()> {
    let con_window = GetConsoleWindow();
    if con_window.0 != 0 {
        ShowWindow(con_window, SW_HIDE);
    }

    let instance = GetModuleHandleA(None);

    if instance.0 == 0 {
        return Err(anyhow!("instance.0 == 0"));
    }

    let window_class = "ncclient";

    let wc = WNDCLASSA {
        hCursor: LoadCursorW(None, IDC_ARROW),
        hInstance: instance,
        hIcon: LoadIconW(instance, "ICON_1"),
        lpszClassName: PSTR(b"ncclient\0".as_ptr() as _),
        // lpszClassName: PSTR(encodeu8(window_class).as_ptr() as _),
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
        "NCWindow",
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

    let mut nid = create_nid(hwnd, instance, "ICON_4");
    P_NID_OFFLINE = &mut nid;
    let mut nid = create_nid(hwnd, instance, "ICON_3");
    P_NID_ERROR = &mut nid;
    let mut nid = create_nid(hwnd, instance, "ICON_2");
    P_NID_LOAD = &mut nid;
    let mut nid = create_nid(hwnd, instance, "ICON_1");
    P_NID_NORMAL = &mut nid;
    P_NID = P_NID_NORMAL;

    let tmp = LoadMenuW(instance, "ID_NC_CONTROL");
    P_HMENU = &mut GetSubMenu(tmp, 0);

    Shell_NotifyIconW(NIM_ADD, P_NID);
    ShowWindow(hwnd, SW_HIDE);

    let mut message = MSG::default();

    std::thread::spawn(move || loop {
        match icon_rx.recv() {
            Ok(IconChange::Normal) => {
                P_NID = P_NID_NORMAL;
                Shell_NotifyIconW(NIM_MODIFY, P_NID);
            }
            Ok(IconChange::Load) => {
                P_NID = P_NID_LOAD;
                Shell_NotifyIconW(NIM_MODIFY, P_NID);
            }
            Ok(IconChange::Error) => {
                P_NID = P_NID_ERROR;
                Shell_NotifyIconW(NIM_MODIFY, P_NID);
            }
            Ok(IconChange::Offline) => {
                P_NID = P_NID_OFFLINE;
                Shell_NotifyIconW(NIM_MODIFY, P_NID);
            }
            _ => break,
        }
    });

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

    Shell_NotifyIconW(NIM_DELETE, P_NID);

    Ok(())
}

fn encode(source: &str) -> Vec<u16> {
    source.encode_utf16().chain(Some(0)).collect()
}

unsafe fn create_nid(hwnd: HWND, instance: HINSTANCE, h_icon_name: &str) -> NOTIFYICONDATAW {
    let mut nid = mem::zeroed::<NOTIFYICONDATAW>();
    nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.uID = ID_MYTRAY;
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.hWnd = hwnd;
    nid.uCallbackMessage = MYMSG_TRAY;
    nid.hIcon = LoadIconW(instance, h_icon_name);
    let mut buf = [0u16; 128];
    let tip = "Nextcloud client app";
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
                        debug!("TASKTRAY ICON CLICKED");
                        let mut p = POINT { x: 0, y: 0 };
                        GetCursorPos(&mut p);
                        // ClientToScreen(window, &mut p);
                        SetForegroundWindow(window);
                        // (*1) I don't know why but TrackPopupMenu function doesn't work in release-build.
                        // If we don't have console and the popupmenu doesn't appear,
                        // we have no way to terminate this app.
                        // So when release-build, console will be appear instead of the popupmenu.
                        TrackPopupMenu(
                            *P_HMENU,
                            TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                            p.x,
                            p.y,
                            0,
                            window,
                            ptr::null_mut(),
                        );
                        PostMessageA(window, WM_NULL, None, None);
                    }
                    _ => return DefWindowProcA(window, message, wparam, lparam),
                }
                LRESULT(0)
            }
            WM_CLOSE | WM_DESTROY | WM_QUERYENDSESSION => {
                if let Some(tx) = P_TX.as_ref() {
                    tx.send(TasktrayMessage::Exit).ok();
                }
                if let Some(tx) = P_NCSYNCMES_TX.as_ref() {
                    tx.send(None).ok();
                }
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_COMMAND => {
                match (wparam.0 as u32, lparam.0 as u32) {
                    (MSGID_SHOWLOG, _) => {
                        debug!("TASKTRAY SHOWLOG");
                        open_notepad(logging::TMPLOGFILENAME.to_string());
                    }
                    (MSGID_EDITCONF, _) => {
                        debug!("TASKTRAY EDITCONF");
                        open_notepad(config::CONFFILENAME.to_string());
                    }
                    (MSGID_EDITEXCLUDE, _) => {
                        debug!("TASKTRAY EDITEXCLUDE");
                        if let Some(tx) = P_TX.as_ref() {
                            tx.send(TasktrayMessage::ExcEdit).ok();
                        }
                    }
                    (MSGID_REPAIR, _) => {
                        debug!("TASKTRAY REPAIR");
                        if let Some(tx) = P_TX.as_ref() {
                            tx.send(TasktrayMessage::Repair).ok();
                        }
                    }
                    (MSGID_RESTART, _) => {
                        debug!("TASKTRAY RESTART");
                        if let Some(tx) = P_TX.as_ref() {
                            tx.send(TasktrayMessage::Restart).ok();

                            if let Some(etx) = P_EMERGENCY_TX.as_ref() {
                                etx.send(true).ok();
                            }
                        }
                    }
                    (MSGID_EXIT, _) => {
                        debug!("TASKTRAY EXIT");
                        if let Some(tx) = P_TX.as_ref() {
                            tx.send(TasktrayMessage::Exit).ok();

                            if let Some(tx) = P_NCSYNCMES_TX.as_ref() {
                                tx.send(None).ok();
                            }
                            if let Some(etx) = P_EMERGENCY_TX.as_ref() {
                                etx.send(false).ok();
                            }

                            PostQuitMessage(0);
                        }
                    }
                    _ => return DefWindowProcA(window, message, wparam, lparam),
                }
                LRESULT(0)
            }
            WM_COPYDATA => {
                let cds = lparam.0 as *mut COPYDATASTRUCT;

                let buf = from_raw_parts((*cds).lpData as *const u8, (*cds).cbData as usize);
                let mes = NCSyncMessage::try_from(buf);
                if let Ok(mes) = mes {
                    debug!("Get Message: {:?}", mes);
                    if let Some(tx) = P_NCSYNCMES_TX.as_ref() {
                        tx.send(Some(mes)).ok();
                    }
                }

                LRESULT(0)
            }
            _ => DefWindowProcA(window, message, wparam, lparam),
        }
    }
}

fn open_notepad(filepath: String) {
    std::thread::spawn(move || {
        std::process::Command::new("notepad")
            .args(&[filepath.as_str()])
            .status()
            .ok();
    });
}
