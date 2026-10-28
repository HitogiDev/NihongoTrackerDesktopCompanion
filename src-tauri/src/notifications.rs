use tauri::AppHandle;

#[cfg(windows)]
pub fn register(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs;
    use tauri::Manager;

    let data_dir = app.path().app_data_dir()?;
    fs::create_dir_all(&data_dir)?;
    let icon = data_dir.join("notification.ico");
    fs::write(&icon, include_bytes!("../icons/icon.ico"))?;
    register_identity(&app.config().identifier,
        app.config().product_name.as_deref().unwrap_or("NihongoTracker Companion"), &icon)?;
    let identifier = app.config().identifier.clone();
    let display_name = app.config().product_name.clone().unwrap_or_else(|| "NihongoTracker Companion".into());
    let executable = std::env::current_exe()?;
    std::thread::spawn(move || -> windows::core::Result<()> {
        let _com = ComApartment::new()?;
        let shortcut = programs_folder()?.join(format!("{display_name}.lnk"));
        register_shortcut(&shortcut, &executable, &identifier, &display_name, &icon)?;
        unsafe {
            windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
                &windows::core::HSTRING::from(identifier),
            )
        }
    }).join().map_err(|_| std::io::Error::other("Notification registration thread failed"))??;
    Ok(())
}

#[cfg(windows)]
struct ComApartment;

#[cfg(windows)]
impl ComApartment {
    fn new() -> windows::core::Result<Self> {
        unsafe {
            windows::Win32::System::Com::CoInitializeEx(None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self)
    }
}

#[cfg(windows)]
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::Com::CoUninitialize(); }
    }
}

#[cfg(windows)]
const APP_USER_MODEL_ID: windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY =
    windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3), pid: 5,
    };

#[cfg(windows)]
fn programs_folder() -> windows::core::Result<std::path::PathBuf> {
    use windows::Win32::{System::Com::CoTaskMemFree, UI::Shell::{SHGetKnownFolderPath, FOLDERID_Programs, KF_FLAG_DEFAULT}};
    unsafe {
        let folder = SHGetKnownFolderPath(&FOLDERID_Programs, KF_FLAG_DEFAULT, None)?;
        let path = folder.to_string();
        CoTaskMemFree(Some(folder.0.cast()));
        Ok(std::path::PathBuf::from(path?))
    }
}

#[cfg(windows)]
fn register_shortcut(shortcut: &std::path::Path, executable: &std::path::Path,
    identifier: &str, display_name: &str, icon: &std::path::Path) -> windows::core::Result<()> {
    use windows::{core::{HSTRING, Interface, PROPVARIANT}, Win32::{
        System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER},
        UI::Shell::{IShellLinkW, ShellLink, SHChangeNotify, SHCNE_CREATE, SHCNE_UPDATEITEM,
            SHCNF_PATHW, SHCNF_FLUSH, PropertiesSystem::IPropertyStore},
    }};
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(executable.as_os_str()))?;
        if let Some(parent) = executable.parent() {
            link.SetWorkingDirectory(&HSTRING::from(parent.as_os_str()))?;
        }
        link.SetDescription(&HSTRING::from(display_name))?;
        link.SetIconLocation(&HSTRING::from(icon.as_os_str()), 0)?;
        let properties: IPropertyStore = link.cast()?;
        properties.SetValue(&APP_USER_MODEL_ID, &PROPVARIANT::from(identifier))?;
        properties.Commit()?;
        let persisted: IPersistFile = link.cast()?;
        let existed = shortcut.exists();
        let path = HSTRING::from(shortcut.as_os_str());
        persisted.Save(&path, true)?;
        // Windows resolves toast sender names from registered Start menu shortcuts.
        SHChangeNotify(if existed { SHCNE_UPDATEITEM } else { SHCNE_CREATE },
            SHCNF_PATHW | SHCNF_FLUSH, Some(path.as_ptr().cast()), None);
    }
    Ok(())
}

#[cfg(windows)]
fn register_identity(identifier: &str, display_name: &str, icon: &std::path::Path) -> std::io::Result<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let (identity, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(
        format!("Software\\Classes\\AppUserModelId\\{identifier}"),
    )?;
    identity.set_value("DisplayName", &display_name)?;
    identity.set_value("IconUri", &icon.to_string_lossy().as_ref())?;
    Ok(())
}

pub fn show(app: &AppHandle, title: &str, body: &str) {
    #[cfg(windows)]
    let result = tauri_winrt_notification::Toast::new(&app.config().identifier)
        .title(title).text1(body).show().map_err(|error| error.to_string());
    #[cfg(not(windows))]
    let result = {
        use tauri_plugin_notification::NotificationExt;
        app.notification().builder().title(title).body(body).show().map_err(|error| error.to_string())
    };
    if let Err(error) = result { log::warn!("Could not show notification: {error}"); }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use winreg::{enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS}, RegKey};

    #[test]
    fn notification_identity_has_the_companion_name_and_a_valid_icon() {
        let identifier = format!("app.nihongotracker.companion.notification-test-{}", std::process::id());
        let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/icon.ico");
        register_identity(&identifier, "NihongoTracker Companion", &icon).unwrap();
        let registry = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Software\\Classes\\AppUserModelId", KEY_ALL_ACCESS).unwrap();
        let identity = registry.open_subkey(&identifier).unwrap();
        let name: String = identity.get_value("DisplayName").unwrap();
        let uri: String = identity.get_value("IconUri").unwrap();
        drop(identity);
        registry.delete_subkey(&identifier).unwrap();
        assert_eq!(name, "NihongoTracker Companion");
        assert_eq!(std::path::PathBuf::from(uri), icon);
        assert_eq!(&std::fs::read(icon).unwrap()[..4], &[0, 0, 1, 0]);
    }

    #[test]
    fn notification_shortcut_persists_the_toast_identity_and_icon() {
        use windows::{core::{BSTR, HSTRING, Interface}, Win32::{
            System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ},
            UI::Shell::{IShellLinkW, ShellLink, PropertiesSystem::IPropertyStore},
        }};
        let _com = ComApartment::new().unwrap();
        let shortcut = std::env::temp_dir().join(format!("ntdc-notification-test-{}.lnk", std::process::id()));
        let executable = std::env::current_exe().unwrap();
        let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/icon.ico");
        register_shortcut(&shortcut, &executable, "app.nihongotracker.companion", "NihongoTracker Companion", &icon).unwrap();
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            let persisted: IPersistFile = link.cast().unwrap();
            persisted.Load(&HSTRING::from(shortcut.as_os_str()), STGM_READ).unwrap();
            let properties: IPropertyStore = link.cast().unwrap();
            let identifier = BSTR::try_from(&properties.GetValue(&APP_USER_MODEL_ID).unwrap()).unwrap().to_string();
            let mut description = [0u16; 256];
            link.GetDescription(&mut description).unwrap();
            let mut icon_path = [0u16; 1024];
            let mut icon_index = 0;
            link.GetIconLocation(&mut icon_path, &mut icon_index).unwrap();
            std::fs::remove_file(&shortcut).unwrap();
            assert_eq!(identifier, "app.nihongotracker.companion");
            assert_eq!(String::from_utf16_lossy(&description[..description.iter().position(|c| *c == 0).unwrap()]), "NihongoTracker Companion");
            assert_eq!(std::path::PathBuf::from(String::from_utf16_lossy(&icon_path[..icon_path.iter().position(|c| *c == 0).unwrap()])), icon);
            assert_eq!(icon_index, 0);
        }
    }

    #[test]
    #[ignore = "Registers NTDC in the current user's Start menu and sends a test notification"]
    fn register_and_send_windows_notification() {
        let _com = ComApartment::new().unwrap();
        let executable = std::path::PathBuf::from(std::env::var_os("NTDC_NOTIFICATION_TEST_EXECUTABLE").expect("Set the executable to register"));
        assert!(executable.is_file());
        let identifier = "app.nihongotracker.companion";
        let display_name = "NihongoTracker Companion";
        let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/icon.ico");
        register_identity(identifier, display_name, &icon).unwrap();
        register_shortcut(&programs_folder().unwrap().join(format!("{display_name}.lnk")),
            &executable, identifier, display_name, &icon).unwrap();
        tauri_winrt_notification::Toast::new(identifier).title(display_name)
            .text1("Registro de notificaciones actualizado.").show().unwrap();
    }
}
