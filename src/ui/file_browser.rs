//! Inline browser for callers that own the selection and subsequent validation.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};

const MAX_ENTRIES: usize = 500;
static LISTING_BUSY: AtomicBool = AtomicBool::new(false);

struct ListingPermit(&'static AtomicBool);

impl Drop for ListingPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn acquire_listing(busy: &'static AtomicBool) -> Option<ListingPermit> {
    busy.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .ok()
        .map(|_| ListingPermit(busy))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserMode {
    File,
    Directory,
    FileAndDirectory,
}

#[derive(Debug)]
struct Entry {
    path: PathBuf,
    directory: bool,
}

#[derive(Debug)]
struct Listing {
    entries: Vec<Entry>,
    truncated: bool,
    omitted: bool,
}

fn list_page(cursor: &mut std::fs::ReadDir) -> Listing {
    let mut entries = Vec::new();
    let mut truncated = true;
    let mut omitted = false;
    for _ in 0..MAX_ENTRIES {
        let Some(item) = cursor.next() else {
            truncated = false;
            break;
        };
        let item = match item {
            Ok(item) => item,
            Err(_) => {
                omitted = true;
                continue;
            }
        };
        // Follow symlinks so directory links can be navigated and file links selected.
        let metadata = match std::fs::metadata(item.path()) {
            Ok(metadata) => metadata,
            Err(_) => {
                omitted = true;
                continue;
            }
        };
        if metadata.is_dir() || metadata.is_file() {
            entries.push(Entry {
                path: item.path(),
                directory: metadata.is_dir(),
            });
        }
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
    });
    Listing {
        entries,
        truncated,
        omitted,
    }
}

struct Cursor {
    entries: std::fs::ReadDir,
    directory: std::fs::File,
}

#[cfg(unix)]
fn open_directory(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::open(path)
}

#[cfg(windows)]
fn open_directory(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().read(true)
        .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

#[cfg(unix)]
fn identity(file: &std::fs::File) -> std::io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    if !metadata.is_dir() {
        return Err(std::io::Error::other("Not a directory"));
    }
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn identity(file: &std::fs::File) -> std::io::Result<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let index = ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64;
    if !file.metadata()?.is_dir() || index == 0 {
        return Err(std::io::Error::other("Directory identity unavailable"));
    }
    Ok((info.dwVolumeSerialNumber as u64, index))
}

#[cfg(not(any(unix, windows)))]
fn open_directory(_path: &Path) -> std::io::Result<std::fs::File> {
    Err(std::io::Error::other("Directory identity unavailable"))
}

#[cfg(not(any(unix, windows)))]
fn identity(_file: &std::fs::File) -> std::io::Result<(u64, u64)> {
    Err(std::io::Error::other("Directory identity unavailable"))
}

fn check_directory(path: &Path, directory: &std::fs::File) -> std::io::Result<()> {
    if identity(directory)? != identity(&open_directory(path)?)? {
        return Err(std::io::Error::other("Directory identity changed"));
    }
    Ok(())
}

fn checked_page(path: &Path, cursor: &mut Cursor) -> std::io::Result<Listing> {
    check_directory(path, &cursor.directory).map_err(refresh_error)?;
    let listing = list_page(&mut cursor.entries);
    check_directory(path, &cursor.directory).map_err(refresh_error)?;
    Ok(listing)
}

fn refresh_error(error: std::io::Error) -> std::io::Error {
    std::io::Error::other(format!("Directory changed or cannot be verified; refresh. / Папка изменилась или её нельзя проверить; обновите список. ({error})"))
}

type Page = (PathBuf, Listing, Cursor);

fn load_directory(path: &Path) -> std::io::Result<Page> {
    let path = std::fs::canonicalize(path)?;
    let directory = open_directory(&path).map_err(refresh_error)?;
    let entries = std::fs::read_dir(&path).map_err(refresh_error)?;
    let mut cursor = Cursor { entries, directory };
    // shortcut: replace-and-restore races and changes after display remain possible; use handle-relative operations for stronger guarantees.
    let listing = checked_page(&path, &mut cursor)?;
    Ok((path, listing, cursor))
}

/// Own one instance per active picker. `show` returns a result only once.
/// Callers must validate the returned path and their own operation/device generation.
pub(crate) struct FileBrowser {
    directory: PathBuf,
    mode: BrowserMode,
    listing: Option<Listing>,
    error: Option<String>,
    pending: Option<Receiver<std::io::Result<Page>>>,
    cursor: Option<Cursor>,
}

pub(crate) enum BrowserResult {
    Selected(PathBuf),
    Directory(PathBuf),
    Cancelled,
}

impl FileBrowser {
    /// `initial_directory` is supplied by the caller without filesystem access on the UI thread.
    pub(crate) fn new(initial_directory: PathBuf, mode: BrowserMode, ctx: &egui::Context) -> Self {
        let mut browser = Self {
            directory: initial_directory,
            mode,
            listing: None,
            error: None,
            pending: None,
            cursor: None,
        };
        browser.load(ctx);
        browser
    }

    fn load(&mut self, ctx: &egui::Context) {
        self.pending = None;
        self.cursor = None;
        self.request_page(ctx, None);
    }

    fn request_page(&mut self, ctx: &egui::Context, cursor: Option<Cursor>) {
        self.error = None;
        if cursor.is_none() {
            self.listing = None;
        }
        let Some(permit) = acquire_listing(&LISTING_BUSY) else {
            self.cursor = cursor;
            self.error = Some("A previous directory read is still running; try again later. / Предыдущее чтение папки ещё выполняется; повторите позже.".into());
            return;
        };
        self.listing = None;
        let path = self.directory.clone();
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        match std::thread::Builder::new().spawn(move || {
            let result = match cursor {
                Some(mut cursor) => {
                    checked_page(&path, &mut cursor).map(|listing| (path, listing, cursor))
                }
                None => load_directory(&path),
            };
            drop(permit);
            let _ = tx.send(result);
            ctx.request_repaint();
        }) {
            Ok(_) => self.pending = Some(rx),
            Err(_) => {
                self.error = Some(
                    "Could not start directory reader. / Не удалось запустить чтение папки.".into(),
                )
            }
        }
    }

    /// Render inside the caller's existing page; remove the instance after a result.
    pub(crate) fn show(&mut self, ui: &mut egui::Ui, russian: bool) -> Option<BrowserResult> {
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    match result {
                        Ok((directory, listing, cursor)) => {
                            self.directory = directory;
                            self.cursor = Some(cursor);
                            self.listing = Some(listing);
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.error = Some(
                        if russian {
                            "Ошибка фонового чтения"
                        } else {
                            "Directory worker failed"
                        }
                        .into(),
                    );
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        ui.label(self.directory.to_string_lossy().into_owned());
        let mut navigate = None;
        let mut next_page = false;
        let mut result = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.pending.is_none() && self.directory.parent().is_some(),
                    egui::Button::new(if russian { "Вверх" } else { "Up" }),
                )
                .clicked()
            {
                navigate = self.directory.parent().map(Path::to_path_buf);
            }
            if ui
                .add_enabled(
                    self.pending.is_none(),
                    egui::Button::new(if russian {
                        "Обновить"
                    } else {
                        "Refresh"
                    }),
                )
                .clicked()
            {
                navigate = Some(self.directory.clone());
            }
            if ui
                .button(if russian { "Отмена" } else { "Cancel" })
                .clicked()
            {
                result = Some(BrowserResult::Cancelled);
            }
        });
        if self.pending.is_some() {
            ui.label(if russian {
                "Чтение папки…"
            } else {
                "Loading directory…"
            });
        }
        if let Some(error) = &self.error {
            ui.label(format!(
                "{}: {error}",
                if russian {
                    "Не удалось прочитать папку"
                } else {
                    "Cannot read directory"
                }
            ));
        }
        if let Some(listing) = &self.listing {
            if listing.truncated {
                if ui
                    .button(if russian {
                        "Следующие записи"
                    } else {
                        "Next entries"
                    })
                    .clicked()
                {
                    next_page = true;
                }
            }
            if listing.omitted {
                ui.label(if russian {
                    "Некоторые записи недоступны и пропущены."
                } else {
                    "Some entries could not be read and were omitted."
                });
            }
            if self.mode != BrowserMode::File
                && ui
                    .button(if russian {
                        "Выбрать эту папку"
                    } else {
                        "Select this directory"
                    })
                    .clicked()
            {
                result = Some(BrowserResult::Directory(self.directory.clone()));
            }
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for entry in &listing.entries {
                        let name = entry.path.file_name().unwrap_or_default().to_string_lossy();
                        let label = if entry.directory {
                            format!("📁 {name}")
                        } else {
                            name.into_owned()
                        };
                        if ui.button(label).clicked() {
                            if entry.directory {
                                navigate = Some(entry.path.clone());
                            } else if self.mode != BrowserMode::Directory {
                                result = Some(BrowserResult::Selected(entry.path.clone()));
                            }
                        }
                    }
                });
        }
        if result.is_none() {
            if let Some(path) = navigate {
                self.directory = path;
                self.load(ui.ctx());
            } else if next_page {
                if let Some(cursor) = self.cursor.take() {
                    self.request_page(ui.ctx(), Some(cursor));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_navigation_does_not_keep_previous_directory_entries() {
        let permit = acquire_listing(&LISTING_BUSY).expect("listing permit");
        let mut browser = FileBrowser {
            directory: PathBuf::from("new"),
            mode: BrowserMode::Directory,
            listing: Some(Listing { entries: vec![], truncated: false, omitted: false }),
            error: None,
            pending: None,
            cursor: None,
        };
        browser.load(&egui::Context::default());
        assert!(browser.listing.is_none());
        assert!(browser.error.is_some());
        let directory = tempfile::tempdir().unwrap();
        browser.listing = Some(Listing { entries: vec![], truncated: true, omitted: false });
        let (_, _, cursor) = load_directory(directory.path()).unwrap();
        browser.request_page(&egui::Context::default(), Some(cursor));
        assert!(browser.listing.is_some());
        assert!(browser.cursor.is_some());
        drop(permit);
    }

    #[test]
    fn cancelled_read_keeps_worker_slot_until_it_finishes() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let permit = acquire_listing(&BUSY).unwrap();
        assert!(acquire_listing(&BUSY).is_none());
        drop(permit);
        assert!(acquire_listing(&BUSY).is_some());
    }

    #[test]
    fn initial_relative_directory_resolves_for_parent_navigation() {
        let (initial, _, _) = load_directory(Path::new(".")).unwrap();
        assert!(initial.is_absolute());
        if let Some(parent) = initial.parent() {
            assert!(!parent.as_os_str().is_empty());
            let (parent, _, _) = load_directory(parent).unwrap();
            assert!(parent.is_absolute());
        }
        let root = initial.ancestors().last().unwrap();
        assert!(root.parent().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn broken_link_does_not_hide_valid_entries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("valid"), b"").unwrap();
        std::os::unix::fs::symlink("missing", dir.path().join("broken")).unwrap();
        let (_, listing, _) = load_directory(dir.path()).unwrap();
        assert!(listing.omitted);
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].path.file_name().unwrap(), "valid");
    }

    #[test]
    fn renamed_or_replaced_directory_requires_refresh() {
        for replace in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("listing");
            std::fs::create_dir(&root).unwrap();
            let (path, _, mut cursor) = load_directory(&root).unwrap();
            std::fs::rename(&root, temporary.path().join("old")).unwrap();
            if replace {
                std::fs::create_dir(&root).unwrap();
            }
            let error = checked_page(&path, &mut cursor).unwrap_err().to_string();
            assert!(error.contains("refresh"), "{error}");
        }
    }

    #[test]
    fn listing_is_bounded_and_reports_errors() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("listing");
        std::fs::create_dir(&root).unwrap();
        for n in 0..=MAX_ENTRIES {
            std::fs::write(root.join(n.to_string()), b"").unwrap();
        }
        let (_, listing, mut cursor) = load_directory(&root).unwrap();
        assert_eq!(listing.entries.len(), MAX_ENTRIES);
        assert!(listing.truncated);
        let first: std::collections::HashSet<_> = listing
            .entries
            .into_iter()
            .map(|entry| entry.path)
            .collect();
        let next = checked_page(&root, &mut cursor).unwrap();
        assert_eq!(next.entries.len(), 1);
        assert!(!next.truncated);
        assert!(!first.contains(&next.entries[0].path));
        assert!(next.entries[0].path.is_file());
        assert!(checked_page(&root, &mut cursor).unwrap().entries.is_empty());
        drop(cursor);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(load_directory(&root).is_err());
    }
}
