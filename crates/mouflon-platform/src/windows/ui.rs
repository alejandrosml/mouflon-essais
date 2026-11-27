//! Ce que les fenêtres de Mouflon ont en commun sous Windows (les réglages,
//! la galerie, « À propos ») : un [`Panel`], fenêtre ordinaire dont le
//! contenu, des contrôles de Windows posés sur des cadres, défile quand il
//! est plus haut que l'écran.
//!
//! Le panneau s'occupe de ce qu'une boîte de dialogue ferait d'elle-même :
//! l'échelle de l'écran (tout se mesure en points, puis se met en pixels
//! pour l'écran où la fenêtre se trouve), l'apparence claire ou sombre de
//! Windows, le clavier (Tab, flèches, Échap : voir [`on_message`]) et la
//! molette. Les contrôles sont ceux de Windows : ils se lisent à la loupe et
//! au lecteur d'écran, et se manient au clavier comme partout ailleurs.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::rc::{Rc, Weak};

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmFlush, DwmGetWindowAttribute,
    DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CreateCompatibleBitmap,
    CreateCompatibleDC, CreateDIBSection, CreateFontIndirectW, CreatePen, CreateSolidBrush,
    DFC_BUTTON, DFCS_BUTTONCHECK, DFCS_CHECKED, DFCS_PUSHED, DIB_RGB_COLORS, DT_CALCRECT,
    DT_NOPREFIX, DT_SINGLELINE, DT_WORDBREAK, DeleteDC, DeleteObject, DrawFocusRect,
    DrawFrameControl, DrawTextW, EndPaint, FW_SEMIBOLD, FillRect, GdiFlush, GetDC, GetMonitorInfoW,
    GetSysColor, GetTextMetricsW, HBRUSH, HDC, HFONT, HGDIOBJ, HPEN, LOGFONTW,
    MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromPoint,
    MonitorFromRect, MonitorFromWindow, PAINTSTRUCT, PS_SOLID, RDW_ALLCHILDREN, RDW_ERASE,
    RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow, ReleaseDC, RoundRect, SRCCOPY,
    ScreenToClient, SelectObject, SetBkColor, SetBkMode, SetTextColor, SetViewportOrgEx,
    TEXTMETRICW, TRANSPARENT, UpdateWindow,
};
use windows_sys::Win32::Graphics::Gdi::{DT_CENTER, DT_VCENTER};
use windows_sys::Win32::Storage::Xps::PrintWindow;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::Controls::{
    BP_CHECKBOX, BST_CHECKED, CBS_CHECKEDNORMAL, CBS_UNCHECKEDNORMAL, CDDS_PREPAINT,
    CDRF_DODEFAULT, CDRF_SKIPDEFAULT, CloseThemeData, DrawThemeBackground, GetThemePartSize,
    ICC_BAR_CLASSES, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, IsAppThemed,
    IsThemeActive, NM_CUSTOMDRAW, NMCUSTOMDRAW, NMHDR, SetScrollInfo, SetWindowTheme,
    TBM_SETPAGESIZE, TBM_SETPOS, TBM_SETRANGE, TBS_AUTOTICKS, TBS_HORZ, TS_DRAW,
};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_FOCUS, ODS_NOFOCUSRECT};
use windows_sys::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForWindow, GetSystemMetricsForDpi, OpenThemeDataForDpi,
    SystemParametersInfoForDpi,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
#[cfg(test)]
use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BM_GETSTATE, BS_AUTOCHECKBOX, BS_MULTILINE, BS_NOTIFY, BS_PUSHBUTTON, BS_TOP, BST_FOCUS,
    BST_PUSHED, CB_ADDSTRING, CB_GETDROPPEDSTATE, CBS_DROPDOWNLIST, CBS_HASSTRINGS, CS_HREDRAW,
    CS_VREDRAW, CreateWindowExW, DestroyWindow, GA_ROOT, GW_CHILD, GW_HWNDNEXT, GWL_EXSTYLE,
    GWL_STYLE, GetAncestor, GetClassNameW, GetClientRect, GetCursorPos, GetForegroundWindow,
    GetNextDlgTabItem, GetScrollInfo, GetWindow, GetWindowLongW, GetWindowRect, HMENU,
    HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW, IDCANCEL, IDI_APPLICATION, IMAGE_ICON, IsChild,
    IsDialogMessageW, IsIconic, LR_DEFAULTSIZE, LR_SHARED, LoadCursorW, LoadIconW, LoadImageW,
    MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MINMAXINFO, MSG, MessageBoxW, NONCLIENTMETRICSW,
    PW_RENDERFULLCONTENT, PostMessageW, RegisterClassExW, SB_BOTTOM, SB_LINEDOWN, SB_LINEUP,
    SB_PAGEDOWN, SB_PAGEUP, SB_THUMBPOSITION, SB_THUMBTRACK, SB_TOP, SB_VERT, SCROLLINFO, SIF_ALL,
    SIF_PAGE, SIF_POS, SIF_RANGE, SM_CXSMICON, SM_CXVSCROLL, SM_CYSMICON, SPI_GETHIGHCONTRAST,
    SPI_GETNONCLIENTMETRICS, SW_ERASE, SW_INVALIDATE, SW_RESTORE, SW_SCROLLCHILDREN, SW_SHOWNORMAL,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, ScrollWindowEx,
    SendMessageW, SetForegroundWindow, SetWindowPos, ShowWindow, SystemParametersInfoW,
    UISF_HIDEFOCUS, WA_INACTIVE, WHEEL_DELTA, WM_ACTIVATE, WM_APP, WM_CHANGEUISTATE, WM_CLOSE,
    WM_COMMAND, WM_CTLCOLORBTN, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DPICHANGED,
    WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYFIRST, WM_KEYLAST, WM_MOUSEWHEEL,
    WM_NCDESTROY, WM_NOTIFY, WM_PAINT, WM_PRINTCLIENT, WM_QUERYUISTATE, WM_SETFOCUS, WM_SETFONT,
    WM_SETTINGCHANGE, WM_SYSCOLORCHANGE, WM_THEMECHANGED, WM_USER, WM_VSCROLL, WNDCLASSEXW,
    WNDPROC, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_CONTROLPARENT, WS_MINIMIZEBOX,
    WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BS_OWNERDRAW, CallNextHookEx, IDC_HAND, SetCursor, SetWindowsHookExW, WH_GETMESSAGE,
    WM_DRAWITEM, WM_NULL, WM_SETCURSOR,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SPI_GETKEYBOARDCUES, UIS_CLEAR, UIS_SET, UISF_HIDEACCEL,
};
use windows_sys::core::w;

use super::{from_wide, wide};

/// Styles d'un texte posé dans une fenêtre (contrôle « Static ») : sans
/// « & » qui soulignerait la lettre suivante ; centré.
const SS_NOPREFIX: u32 = 0x80;
const SS_CENTER: u32 = 0x01;
/// La place du curseur d'un contrôle à crans.
pub(super) const TBM_GETPOS: u32 = WM_USER;
/// Le panneau, qui vient de quitter le premier plan, se range sous la
/// fenêtre qui l'a pris.
const WM_APP_LOWER: u32 = WM_APP + 2;
/// Cases et boutons survolés (`BM_GETSTATE`).
const BST_HOT: u32 = 0x200;
/// Les thèmes sombres des contrôles de Windows existent à partir de cette
/// version de Windows 10 (la 1903).
const DARK_SINCE: u32 = 18362;

/// Ouvre un dossier, un fichier ou une adresse avec ce que Windows prévoit
/// pour lui (l'Explorateur, le navigateur, les Paramètres…).
pub(super) fn open(target: &str) -> bool {
    let (verb, target) = (wide("open"), wide(target));
    // SAFETY: les deux textes vivent jusqu'au retour ; ni fenêtre, ni
    // arguments, ni dossier de départ.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // (Au-delà de 32 : c'est fait.)
    result as usize > 32
}

/// Un message à l'utilisateur, dans une petite boîte qui attend son « OK » :
/// pour ce qui empêche Mouflon de démarrer.
pub fn message(title: &str, text: &str) {
    let (title, text) = (wide(title), wide(text));
    // SAFETY: les deux textes vivent jusqu'au retour ; pas de fenêtre parente.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND,
        );
    }
}

// ——— L'apparence ———

/// L'apparence d'une fenêtre : claire, sombre, ou celle d'un thème à fort
/// contraste (les couleurs que l'utilisateur a choisies, sans rien y
/// ajouter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Look {
    Light,
    Dark,
    Contrast,
}

/// Les couleurs d'un panneau (rouge, vert et bleu, comme Windows les note).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Colors {
    /// Le fond de la fenêtre, et celui des cadres posés dessus.
    pub window: COLORREF,
    pub card: COLORREF,
    /// Le bord des cadres, le trait entre deux lignes d'un cadre.
    pub edge: COLORREF,
    pub line: COLORREF,
    /// Le texte, le texte qui explique, les liens.
    pub text: COLORREF,
    pub dim: COLORREF,
    pub link: COLORREF,
}

const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    red as u32 | (green as u32) << 8 | (blue as u32) << 16
}

impl Look {
    /// Les couleurs de Windows 11 pour ses propres réglages, en clair et en
    /// sombre ; à fort contraste, celles du thème.
    pub(super) fn colors(self) -> Colors {
        match self {
            Self::Light => Colors {
                window: rgb(243, 243, 243),
                card: rgb(251, 251, 251),
                edge: rgb(229, 229, 229),
                line: rgb(234, 234, 234),
                text: rgb(27, 27, 27),
                dim: rgb(95, 95, 95),
                link: rgb(0, 95, 184),
            },
            Self::Dark => Colors {
                window: rgb(32, 32, 32),
                card: rgb(43, 43, 43),
                edge: rgb(58, 58, 58),
                line: rgb(56, 56, 56),
                text: rgb(255, 255, 255),
                dim: rgb(200, 200, 200),
                link: rgb(96, 205, 255),
            },
            Self::Contrast => {
                // SAFETY: lecture de couleurs du système, par leur numéro.
                let color = |index| unsafe { GetSysColor(index) };
                // COLOR_WINDOW, COLOR_WINDOWTEXT, COLOR_HOTLIGHT.
                let (window, text, link) = (color(5), color(8), color(26));
                Colors {
                    window,
                    card: window,
                    edge: text,
                    line: text,
                    text,
                    dim: text,
                    link,
                }
            }
        }
    }
}

/// Un nombre du registre de Windows.
fn registry_number(root: HKEY, key: &str, name: &str) -> Option<u32> {
    let (key, name) = (wide(key), wide(name));
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: les deux textes vivent jusqu'au retour ; `value` est à nous,
    // donné avec sa taille, et Windows n'y écrit qu'un nombre
    // (`RRF_RT_REG_DWORD`).
    let status = unsafe {
        RegGetValueW(
            root,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut value).cast::<c_void>(),
            &mut size,
        )
    };
    (status == 0).then_some(value)
}

/// Le numéro de fabrication de ce Windows (22000 et plus : Windows 11).
pub(super) fn windows_build() -> u32 {
    let key = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let name = wide("CurrentBuildNumber");
    let mut buffer = [0u16; 16];
    let mut size = size_of_val(&buffer) as u32;
    // SAFETY: les deux textes vivent jusqu'au retour ; `buffer` est à nous,
    // donné avec sa taille en octets ; Windows n'y écrit qu'un texte terminé
    // par zéro (`RRF_RT_REG_SZ`).
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast::<c_void>(),
            &mut size,
        )
    };
    if status != 0 {
        return 0;
    }
    from_wide(&buffer).trim().parse().unwrap_or(0)
}

/// L'apparence que l'utilisateur a choisie pour ses apps dans Windows.
pub(super) fn system_look() -> Look {
    let mut contrast = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        dwFlags: 0,
        lpszDefaultScheme: std::ptr::null_mut(),
    };
    // SAFETY: `contrast` porte sa taille ; l'appel ne fait que l'écrire. Les
    // deux questions suivantes n'ont pas d'argument.
    let (high, themed) = unsafe {
        let known = SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            (&raw mut contrast).cast::<c_void>(),
            0,
        ) != 0;
        (
            known && contrast.dwFlags & HCF_HIGHCONTRASTON != 0,
            IsAppThemed() != 0 && IsThemeActive() != 0,
        )
    };
    if high {
        return Look::Contrast;
    }
    let light = registry_number(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "AppsUseLightTheme",
    );
    // En sombre seulement là où les contrôles de Windows savent l'être (et
    // se dessinent par thème : sinon leurs textes resteraient noirs).
    if light == Some(0) && themed && windows_build() >= DARK_SINCE {
        Look::Dark
    } else {
        Look::Light
    }
}

/// Les pinceaux d'un panneau, à ses couleurs.
pub(super) struct Paint {
    pub look: Look,
    pub colors: Colors,
    pub window: HBRUSH,
    pub card: HBRUSH,
    line: HBRUSH,
    edge: HPEN,
}

impl Paint {
    fn new(look: Look) -> Self {
        let colors = look.colors();
        // SAFETY: création d'objets de dessin, rendus à Windows par `Drop`.
        unsafe {
            Self {
                look,
                colors,
                window: CreateSolidBrush(colors.window),
                card: CreateSolidBrush(colors.card),
                line: CreateSolidBrush(colors.line),
                edge: CreatePen(PS_SOLID, 1, colors.edge),
            }
        }
    }
}

impl Drop for Paint {
    fn drop(&mut self) {
        // SAFETY: ces objets sont à nous, et plus personne ne dessine avec
        // (le panneau qui les portait en a de nouveaux, ou n'est plus).
        unsafe {
            DeleteObject(self.window as HGDIOBJ);
            DeleteObject(self.card as HGDIOBJ);
            DeleteObject(self.line as HGDIOBJ);
            DeleteObject(self.edge as HGDIOBJ);
        }
    }
}

/// Les polices d'un panneau, à l'échelle de son écran : celle des messages
/// de Windows, en maigre, en gras, et en grand pour un titre.
pub(super) struct Fonts {
    pub text: HFONT,
    pub bold: HFONT,
    pub title: HFONT,
    /// Soulignée : celle des liens.
    link: HFONT,
    /// Hauteur d'une ligne de texte, en pixels.
    pub line: i32,
}

impl Fonts {
    fn new(dpi: u32) -> Self {
        let mut metrics = NONCLIENTMETRICSW {
            cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
            ..NONCLIENTMETRICSW::default()
        };
        // SAFETY: `metrics` porte sa taille ; l'appel ne fait que l'écrire.
        let known = unsafe {
            SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS,
                metrics.cbSize,
                (&raw mut metrics).cast::<c_void>(),
                0,
                dpi,
            ) != 0
        };
        let mut face: LOGFONTW = metrics.lfMessageFont;
        if !known || face.lfHeight == 0 {
            // Faute de mieux : Segoe UI, neuf points.
            face = LOGFONTW::default();
            face.lfHeight = -(12 * dpi as i32 + 48) / 96;
            for (slot, unit) in face.lfFaceName.iter_mut().zip("Segoe UI".encode_utf16()) {
                *slot = unit;
            }
        }
        let mut bold = face;
        bold.lfWeight = FW_SEMIBOLD as i32;
        let mut title = bold;
        title.lfHeight = face.lfHeight * 8 / 5;
        let mut link = face;
        link.lfUnderline = 1;
        // SAFETY: chaque description de police est à nous et complète ; les
        // polices sont rendues à Windows par `Drop`.
        let (text, bold, title, link) = unsafe {
            (
                CreateFontIndirectW(&face),
                CreateFontIndirectW(&bold),
                CreateFontIndirectW(&title),
                CreateFontIndirectW(&link),
            )
        };
        let mut fonts = Self {
            text,
            bold,
            title,
            link,
            line: (16 * dpi as i32 + 48) / 96,
        };
        fonts.line = line_height(text).unwrap_or(fonts.line);
        fonts
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        // SAFETY: ces polices sont à nous ; les contrôles qui s'en servaient
        // en ont reçu d'autres, ou n'existent plus.
        unsafe {
            DeleteObject(self.text as HGDIOBJ);
            DeleteObject(self.bold as HGDIOBJ);
            DeleteObject(self.title as HGDIOBJ);
            DeleteObject(self.link as HGDIOBJ);
        }
    }
}

/// Hauteur d'une ligne de texte dans cette police.
fn line_height(font: HFONT) -> Option<i32> {
    let mut metrics = TEXTMETRICW::default();
    // SAFETY: le contexte de l'écran est pris et rendu ici ; la police est
    // vivante ; `metrics` est à nous.
    let known = unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let old = SelectObject(dc, font as HGDIOBJ);
        let known = GetTextMetricsW(dc, &mut metrics) != 0;
        SelectObject(dc, old);
        ReleaseDC(std::ptr::null_mut(), dc);
        known
    };
    (known && metrics.tmHeight > 0).then_some(metrics.tmHeight)
}

/// La taille d'un texte dans une police : sur une ligne, ou mis en page sur
/// `width` pixels (il passe à la ligne entre deux mots).
fn measure(font: HFONT, text: &str, width: Option<i32>) -> (i32, i32) {
    if text.is_empty() {
        return (0, 0);
    }
    let text = wide(text);
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: width.unwrap_or(0).max(0),
        bottom: 0,
    };
    let flags = DT_CALCRECT
        | DT_NOPREFIX
        | if width.is_some() {
            DT_WORDBREAK
        } else {
            DT_SINGLELINE
        };
    // SAFETY: le contexte de l'écran est pris et rendu ici ; la police est
    // vivante ; le texte (terminé par zéro) et `rect` vivent jusqu'au retour.
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let old = SelectObject(dc, font as HGDIOBJ);
        DrawTextW(dc, text.as_ptr(), -1, &mut rect, flags);
        SelectObject(dc, old);
        ReleaseDC(std::ptr::null_mut(), dc);
    }
    (rect.right - rect.left, rect.bottom - rect.top)
}

/// Un « & » dans le libellé d'un bouton y soulignerait la lettre suivante :
/// doublé, il se montre tel quel.
pub(super) fn escaped(label: &str) -> String {
    label.replace('&', "&&")
}

// ——— Le défilement ———

/// Où va le défilement quand on demande `wanted` : jamais avant le début
/// du contenu, ni au-delà de sa fin.
pub(super) fn scroll_within(wanted: i32, content: i32, view: i32) -> i32 {
    wanted.clamp(0, (content - view).max(0))
}

/// De combien défiler pour qu'un contrôle (de `top` à `bottom`, dans la
/// vue) se voie en entier, avec `margin` autour si possible.
pub(super) fn scroll_to_reveal(top: i32, bottom: i32, view: i32, margin: i32) -> i32 {
    if top < margin {
        top - margin
    } else if bottom > view - margin {
        // Sans faire sortir son haut par le haut.
        (bottom - (view - margin)).min(top - margin).max(0)
    } else {
        0
    }
}

// ——— Le panneau ———

/// Le ton d'un texte : sa couleur, sa graisse, et le fond où il est posé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Tone {
    pub dim: bool,
    pub bold: bool,
    pub on_card: bool,
}

impl Tone {
    /// Le titre d'une section, sur le fond de la fenêtre.
    pub(super) const HEADER: Self = Self {
        dim: false,
        bold: true,
        on_card: false,
    };
    /// Dans un cadre : un nom, un texte, une explication.
    pub(super) const NAME: Self = Self {
        dim: false,
        bold: true,
        on_card: true,
    };
    pub(super) const TEXT: Self = Self {
        dim: false,
        bold: false,
        on_card: true,
    };
    pub(super) const HELP: Self = Self {
        dim: true,
        bold: false,
        on_card: true,
    };
    /// Sur le fond de la fenêtre : un texte, un texte discret.
    pub(super) const PLAIN: Self = Self {
        dim: false,
        bold: false,
        on_card: false,
    };
    pub(super) const FAINT: Self = Self {
        dim: true,
        bold: false,
        on_card: false,
    };
}

/// Ce que le panneau a fait d'un message de sa fenêtre.
pub(super) enum Handled {
    /// Traité : la réponse à rendre à Windows.
    Done(LRESULT),
    /// Les mesures ont changé (échelle de l'écran, police, apparence) : le
    /// contenu est à remettre en page ; avec la place que Windows propose
    /// pour la fenêtre, s'il en propose une.
    Relayout(Option<RECT>),
    /// Rien pour le panneau.
    No,
}

/// Une fenêtre de Mouflon : voir le module.
pub(super) struct Panel {
    pub hwnd: HWND,
    alive: Cell<bool>,
    dpi: Cell<u32>,
    fonts: RefCell<Rc<Fonts>>,
    paint: RefCell<Rc<Paint>>,
    /// Une apparence imposée (les images de la vitrine).
    forced: Cell<Option<Look>>,
    /// L'apparence est en train d'être appliquée : les messages que cela
    /// fait envoyer à la fenêtre ne la font pas recommencer.
    applying: Cell<bool>,
    /// Taille du contenu, en pixels, et de combien il a défilé.
    content: Cell<(i32, i32)>,
    scroll: Cell<i32>,
    /// La fenêtre peut dépasser de l'écran (les images de la vitrine).
    unbounded: Cell<bool>,
    /// Hauteur la plus grande de ce qu'elle montre, en pixels (0 : celle
    /// de l'écran) : au-delà, le contenu défile.
    max_view: Cell<i32>,
    /// Le ton de chaque texte, le libellé de chaque case.
    tones: RefCell<HashMap<isize, Tone>>,
    checks: RefCell<HashMap<isize, Vec<u16>>>,
    /// Le libellé de chaque lien.
    links: RefCell<HashMap<isize, Vec<u16>>>,
    /// Les cadres et les traits peints sous les contrôles (dans le contenu).
    cards: RefCell<Vec<RECT>>,
    lines: RefCell<Vec<RECT>>,
    /// Le contrôle qui avait le clavier quand la fenêtre a quitté le premier
    /// plan.
    focus: Cell<isize>,
    next_id: Cell<isize>,
    /// Côté de la coche d'une case, en pixels (0 : à mesurer).
    glyph: Cell<i32>,
}

thread_local! {
    /// Les panneaux ouverts (pour le clavier et la molette : voir
    /// [`on_message`]). Jamais défaits : la liste reste lisible jusqu'au
    /// dernier message du fil.
    static PANELS: ManuallyDrop<RefCell<Vec<Weak<Panel>>>> =
        const { ManuallyDrop::new(RefCell::new(Vec::new())) };
}

/// Le panneau de cette fenêtre, si c'en est un.
fn panel_of(hwnd: HWND) -> Option<Rc<Panel>> {
    PANELS.with(|panels| {
        let panels = panels.try_borrow().ok()?;
        panels
            .iter()
            .filter_map(Weak::upgrade)
            .find(|panel| panel.hwnd == hwnd && panel.alive.get())
    })
}

/// L'icône du programme (celle de son fichier), en grand ou en petit ; à
/// défaut, celle que Windows donne à tout programme.
fn app_icon(small: bool, dpi: u32) -> isize {
    // SAFETY: lecture d'une ressource du programme par son numéro (1 : voir
    // `build.rs`), puis d'une icône du système ; les deux sont partagées et
    // n'ont pas à être rendues.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let (width, height, flags) = if small {
            (
                GetSystemMetricsForDpi(SM_CXSMICON, dpi),
                GetSystemMetricsForDpi(SM_CYSMICON, dpi),
                LR_SHARED,
            )
        } else {
            (0, 0, LR_SHARED | LR_DEFAULTSIZE)
        };
        let first = std::ptr::without_provenance::<u16>(1);
        let icon = LoadImageW(instance, first, IMAGE_ICON, width, height, flags);
        if icon.is_null() {
            LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) as isize
        } else {
            icon as isize
        }
    }
}

/// La zone de travail de l'écran où se trouve le curseur (celui où
/// l'utilisateur regarde), ou de l'écran principal.
fn work_under_cursor() -> RECT {
    let mut at = POINT { x: 0, y: 0 };
    // SAFETY: `at` est à nous ; l'écran rendu est celui du point, ou le
    // plus proche.
    let monitor = unsafe {
        GetCursorPos(&mut at);
        MonitorFromPoint(at, MONITOR_DEFAULTTONEAREST)
    };
    work_of(monitor)
}

fn work_of(monitor: *mut c_void) -> RECT {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        rcMonitor: RECT::default(),
        rcWork: RECT::default(),
        dwFlags: 0,
    };
    // SAFETY: `info` porte sa taille ; l'appel ne fait que l'écrire.
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return RECT {
            left: 0,
            top: 0,
            right: 1024,
            bottom: 728,
        };
    }
    info.rcWork
}

/// Les contrôles d'une fenêtre, dans leur ordre.
fn children(hwnd: HWND) -> Vec<HWND> {
    let mut found = Vec::new();
    // SAFETY: parcours des fenêtres enfants d'une fenêtre vivante de ce fil.
    unsafe {
        let mut child = GetWindow(hwnd, GW_CHILD);
        while !child.is_null() {
            found.push(child);
            child = GetWindow(child, GW_HWNDNEXT);
        }
    }
    found
}

fn class_of(hwnd: HWND) -> String {
    let mut class = [0u16; 64];
    // SAFETY: `class` est à nous, donné avec sa taille.
    unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    from_wide(&class)
}

impl Panel {
    /// Crée la fenêtre, cachée, sur l'écran où se trouve le curseur ; ses
    /// messages vont à `proc`, qui les montre à [`Panel::handle`].
    pub(super) fn create(class: &str, title: &str, proc: WNDPROC) -> Option<Rc<Self>> {
        static CONTROLS: std::sync::Once = std::sync::Once::new();
        CONTROLS.call_once(|| {
            let wanted = INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_STANDARD_CLASSES | ICC_BAR_CLASSES,
            };
            // SAFETY: `wanted` porte sa taille ; l'appel déclare les
            // contrôles de Windows dont les panneaux se servent.
            unsafe { InitCommonControlsEx(&wanted) };
        });
        let work = work_under_cursor();
        let (class, title) = (wide(class), wide(title));
        // SAFETY: la classe est déclarée avec la procédure donnée, qui vit
        // autant que le programme ; les textes vivent jusqu'au retour ; ni
        // parent, ni menu, ni données. La fenêtre appartient à ce fil.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let declared = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: proc,
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: app_icon(false, 96) as _,
                hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class.as_ptr(),
                hIconSm: app_icon(true, 96) as _,
            };
            // (Déjà déclarée : la création de la fenêtre le dira.)
            RegisterClassExW(&declared);
            CreateWindowExW(
                WS_EX_CONTROLPARENT,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
                work.left + 60,
                work.top + 60,
                200,
                200,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            log::error!("fenêtre de Mouflon refusée par Windows");
            return None;
        }
        // SAFETY: la fenêtre vient d'être créée sur ce fil.
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(48);
        let panel = Rc::new(Self {
            hwnd,
            alive: Cell::new(true),
            dpi: Cell::new(dpi),
            fonts: RefCell::new(Rc::new(Fonts::new(dpi))),
            paint: RefCell::new(Rc::new(Paint::new(system_look()))),
            forced: Cell::new(None),
            applying: Cell::new(false),
            content: Cell::new((0, 0)),
            scroll: Cell::new(0),
            unbounded: Cell::new(false),
            max_view: Cell::new(0),
            tones: RefCell::default(),
            checks: RefCell::default(),
            links: RefCell::default(),
            cards: RefCell::default(),
            lines: RefCell::default(),
            focus: Cell::new(0),
            next_id: Cell::new(100),
            glyph: Cell::new(0),
        });
        PANELS.with(|panels| {
            let mut panels = panels.borrow_mut();
            panels.retain(|known| known.strong_count() > 0);
            panels.push(Rc::downgrade(&panel));
        });
        watch_messages();
        Some(panel)
    }

    pub(super) fn dpi(&self) -> u32 {
        self.dpi.get()
    }

    /// Des points (96 par pouce, ceux de Windows) en pixels de cet écran.
    pub(super) fn px(&self, points: i32) -> i32 {
        (points * self.dpi.get() as i32 + 48) / 96
    }

    pub(super) fn fonts(&self) -> Rc<Fonts> {
        Rc::clone(&self.fonts.borrow())
    }

    pub(super) fn paint(&self) -> Rc<Paint> {
        Rc::clone(&self.paint.borrow())
    }

    /// Ce que la fenêtre montre de son contenu ne dépasse pas cette
    /// hauteur (au prochain [`Panel::fit`]).
    pub(super) fn set_max_view(&self, pixels: i32) {
        self.max_view.set(pixels);
    }

    fn font(&self, bold: bool) -> HFONT {
        let fonts = self.fonts();
        if bold { fonts.bold } else { fonts.text }
    }

    // ——— Les contrôles ———

    fn child(&self, class: *const u16, text: &str, style: u32, tone: Tone) -> HWND {
        let text = wide(text);
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        // SAFETY: la classe est une de celles de Windows (texte constant) ;
        // le texte vit jusqu'au retour ; le parent est notre fenêtre,
        // vivante, sur ce fil ; le « menu » d'un contrôle est son numéro.
        let hwnd = unsafe {
            let hwnd = CreateWindowExW(
                0,
                class,
                text.as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                0,
                0,
                0,
                0,
                self.hwnd,
                id as HMENU,
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            );
            SendMessageW(hwnd, WM_SETFONT, self.font(tone.bold) as WPARAM, 0);
            hwnd
        };
        self.tones.borrow_mut().insert(hwnd as isize, tone);
        self.theme(hwnd);
        hwnd
    }

    /// Un texte.
    pub(super) fn label(&self, text: &str, tone: Tone) -> HWND {
        self.child(w!("Static"), text, SS_NOPREFIX, tone)
    }

    /// Un texte centré dans sa largeur.
    pub(super) fn centered(&self, text: &str, tone: Tone) -> HWND {
        self.child(w!("Static"), text, SS_NOPREFIX | SS_CENTER, tone)
    }

    /// Un lien (sur le fond de la fenêtre) : un bouton, pour le clavier et
    /// le lecteur d'écran, dessiné comme un lien.
    pub(super) fn link(&self, text: &str) -> HWND {
        let hwnd = self.child(
            w!("Button"),
            &escaped(text),
            BS_OWNERDRAW as u32 | BS_NOTIFY as u32 | WS_TABSTOP,
            Tone::PLAIN,
        );
        let mut label = wide(text);
        label.pop();
        self.links.borrow_mut().insert(hwnd as isize, label);
        hwnd
    }

    /// Une case à cocher (dans un cadre). Son libellé est dessiné par le
    /// panneau, à ses couleurs : celui de Windows resterait noir sur fond
    /// sombre.
    pub(super) fn check(&self, text: &str) -> HWND {
        let hwnd = self.child(
            w!("Button"),
            &escaped(text),
            BS_AUTOCHECKBOX as u32
                | BS_MULTILINE as u32
                | BS_TOP as u32
                | BS_NOTIFY as u32
                | WS_TABSTOP,
            Tone::TEXT,
        );
        let mut label = wide(text);
        label.pop();
        self.checks.borrow_mut().insert(hwnd as isize, label);
        hwnd
    }

    /// Un bouton (dans un cadre).
    pub(super) fn button(&self, text: &str) -> HWND {
        self.child(
            w!("Button"),
            &escaped(text),
            BS_PUSHBUTTON as u32 | BS_NOTIFY as u32 | WS_TABSTOP,
            Tone::TEXT,
        )
    }

    /// Une liste déroulante (dans un cadre), avec ses choix.
    pub(super) fn combo(&self, choices: &[String]) -> HWND {
        let hwnd = self.child(
            w!("ComboBox"),
            "",
            CBS_DROPDOWNLIST as u32 | CBS_HASSTRINGS as u32 | WS_VSCROLL | WS_TABSTOP,
            Tone::TEXT,
        );
        for choice in choices {
            let choice = wide(choice);
            // SAFETY: le texte vit jusqu'au retour (la liste le copie).
            unsafe { SendMessageW(hwnd, CB_ADDSTRING, 0, choice.as_ptr() as LPARAM) };
        }
        hwnd
    }

    /// Un curseur à `steps` crans (dans un cadre).
    pub(super) fn slider(&self, steps: usize) -> HWND {
        let hwnd = self.child(
            w!("msctls_trackbar32"),
            "",
            TBS_HORZ | TBS_AUTOTICKS | WS_TABSTOP,
            Tone::TEXT,
        );
        let last = steps.saturating_sub(1).max(1) as isize;
        // SAFETY: messages sans pointeur à un contrôle vivant de ce fil
        // (étendue de 0 à `last`, un cran par flèche ou par clic).
        unsafe {
            SendMessageW(hwnd, TBM_SETRANGE, 1, last << 16);
            SendMessageW(hwnd, TBM_SETPAGESIZE, 0, 1);
            SendMessageW(hwnd, TBM_SETPOS, 1, 0);
        }
        hwnd
    }

    /// Un texte en grand (le nom de l'app).
    pub(super) fn set_title_font(&self, hwnd: HWND) {
        // SAFETY: comme `set_tone`.
        unsafe { SendMessageW(hwnd, WM_SETFONT, self.fonts().title as WPARAM, 1) };
    }

    // ——— Les mesures ———

    /// La taille d'un texte sur une ligne.
    pub(super) fn text_size(&self, text: &str, bold: bool) -> (i32, i32) {
        measure(self.font(bold), text, None)
    }

    /// La taille d'un texte en grand, sur une ligne.
    pub(super) fn title_size(&self, text: &str) -> (i32, i32) {
        measure(self.fonts().title, text, None)
    }

    /// La hauteur d'un texte mis en page sur `width` pixels.
    pub(super) fn wrapped_height(&self, text: &str, bold: bool, width: i32) -> i32 {
        measure(self.font(bold), text, Some(width)).1
    }

    /// Côté de la coche d'une case, en pixels.
    fn glyph(&self) -> i32 {
        if self.glyph.get() > 0 {
            return self.glyph.get();
        }
        let mut size = SIZE { cx: 0, cy: 0 };
        // SAFETY: le thème est pris et rendu ici ; `size` est à nous.
        unsafe {
            let theme = OpenThemeDataForDpi(self.hwnd, w!("Button"), self.dpi());
            if theme != 0 {
                GetThemePartSize(
                    theme,
                    std::ptr::null_mut(),
                    BP_CHECKBOX,
                    CBS_UNCHECKEDNORMAL,
                    std::ptr::null(),
                    TS_DRAW,
                    &mut size,
                );
                CloseThemeData(theme);
            }
        }
        let side = if size.cx > 0 { size.cx } else { self.px(13) };
        self.glyph.set(side);
        side
    }

    /// De combien le libellé d'une case est en retrait de son bord gauche.
    pub(super) fn check_indent(&self) -> i32 {
        self.glyph() + self.px(8)
    }

    /// La taille d'une case et de son libellé, mis en page sur `width`
    /// pixels au plus.
    pub(super) fn check_size(&self, text: &str, width: i32) -> (i32, i32) {
        let indent = self.check_indent();
        let (w, h) = measure(self.font(false), text, Some((width - indent).max(1)));
        // (Un pixel de marge autour du libellé : son cadre de clavier.)
        (
            (indent + w + 2).min(width),
            h.max(self.glyph()).max(self.fonts().line) + 2,
        )
    }

    /// La taille d'un bouton à ce libellé.
    pub(super) fn button_size(&self, text: &str) -> (i32, i32) {
        let (w, _) = self.text_size(text, false);
        (
            (w + self.px(28)).max(self.px(84)),
            self.fonts().line + self.px(12),
        )
    }

    /// La taille d'une liste déroulante fermée, assez large pour ses choix.
    pub(super) fn combo_size(&self, combo: HWND, choices: &[String]) -> (i32, i32) {
        let widest = choices
            .iter()
            .map(|choice| self.text_size(choice, false).0)
            .max()
            .unwrap_or(0);
        let mut rect = RECT::default();
        // SAFETY: `rect` est à nous ; le contrôle est vivant.
        unsafe { GetWindowRect(combo, &mut rect) };
        let height = rect.bottom - rect.top;
        (
            widest + self.px(40),
            if height > 0 {
                height
            } else {
                self.fonts().line + self.px(10)
            },
        )
    }

    /// La taille d'un lien à ce libellé.
    pub(super) fn link_size(&self, text: &str) -> (i32, i32) {
        let (w, h) = measure(self.fonts().link, text, None);
        (w + self.px(8), h.max(self.fonts().line) + self.px(4))
    }

    /// Hauteur d'un curseur à crans.
    pub(super) fn slider_height(&self) -> i32 {
        self.px(30)
    }

    // ——— La mise en place ———

    /// Pose un contrôle dans le contenu (le défilement est compté ici).
    pub(super) fn place(&self, hwnd: HWND, x: i32, y: i32, width: i32, height: i32) {
        // SAFETY: le contrôle est un enfant vivant de notre fenêtre.
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                x,
                y - self.scroll.get(),
                width.max(0),
                height.max(0),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    /// Une liste déroulante : la hauteur donnée à Windows compte aussi sa
    /// liste ouverte.
    pub(super) fn place_combo(&self, hwnd: HWND, x: i32, y: i32, width: i32, items: usize) {
        let open = self.fonts().line * (items as i32 + 2) + self.px(40);
        self.place(hwnd, x, y, width, open);
    }

    /// Montre ou cache un contrôle.
    pub(super) fn set_shown(&self, hwnd: HWND, shown: bool) {
        // SAFETY: le contrôle est un enfant vivant de notre fenêtre (0 :
        // caché, 8 : montré sans prendre le clavier).
        unsafe { ShowWindow(hwnd, if shown { 8 } else { 0 }) };
    }

    /// Les cadres et les traits à peindre sous les contrôles (places dans
    /// le contenu).
    pub(super) fn set_frames(&self, cards: Vec<RECT>, lines: Vec<RECT>) {
        *self.cards.borrow_mut() = cards;
        *self.lines.borrow_mut() = lines;
    }

    /// Ce que le cadre de la fenêtre (barre de titre, bords) ajoute à son
    /// contenu, en largeur et en hauteur, et la zone de travail de son écran.
    fn chrome(&self) -> (i32, i32, RECT) {
        let mut frame = RECT::default();
        // SAFETY: la fenêtre est vivante, sur ce fil ; `frame` est à nous.
        let work = unsafe {
            let style = GetWindowLongW(self.hwnd, GWL_STYLE) as u32 & !WS_VSCROLL;
            let ex_style = GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32;
            AdjustWindowRectExForDpi(&mut frame, style, 0, ex_style, self.dpi());
            work_of(MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST))
        };
        (frame.right - frame.left, frame.bottom - frame.top, work)
    }

    /// Le plus grand contenu que la fenêtre peut montrer en entier sur son
    /// écran : largeur et hauteur, en pixels.
    pub(super) fn room(&self) -> (i32, i32) {
        if self.unbounded.get() {
            return (i32::MAX / 4, i32::MAX / 4);
        }
        let (chrome_w, chrome_h, work) = self.chrome();
        let margin = self.px(16);
        let mut height = (work.bottom - work.top - chrome_h - margin).max(self.px(160));
        if self.max_view.get() > 0 {
            height = height.min(self.max_view.get());
        }
        (work.right - work.left - chrome_w - margin, height)
    }

    /// La fenêtre prend la taille de son contenu ; plus haut que l'écran, il
    /// défile. Son coin en haut à gauche ne bouge pas (sauf pour rester sur
    /// l'écran).
    pub(super) fn fit(&self, width: i32, height: i32) {
        self.content.set((width, height));
        let dpi = self.dpi();
        let (chrome_w, chrome_h, work) = self.chrome();
        let (_, room) = self.room();
        let mut now = RECT::default();
        // SAFETY: la fenêtre est vivante ; `now` est à nous.
        unsafe { GetWindowRect(self.hwnd, &mut now) };
        let scrolls = height > room && !self.unbounded.get();
        let view = if scrolls { room } else { height };
        let bar = if scrolls {
            // SAFETY: simple question, sans pointeur.
            unsafe { GetSystemMetricsForDpi(SM_CXVSCROLL, dpi) }
        } else {
            0
        };
        let scroll = scroll_within(self.scroll.get(), height, view);
        let info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: (height - 1).max(0),
            nPage: view.max(0) as u32,
            nPos: scroll,
            nTrackPos: 0,
        };
        let (outer_w, outer_h) = (width + bar + chrome_w, view + chrome_h);
        let (mut x, mut y) = (now.left, now.top);
        if !self.unbounded.get() {
            x = x.min(work.right - outer_w).max(work.left);
            y = y.min(work.bottom - outer_h).max(work.top);
        }
        // SAFETY: la fenêtre est vivante, sur ce fil ; `info` porte sa
        // taille. (Une étendue qui tient dans la page retire la barre.)
        unsafe {
            SetScrollInfo(self.hwnd, SB_VERT, &info, 1);
            SetWindowPos(
                self.hwnd,
                std::ptr::null_mut(),
                x,
                y,
                outer_w,
                outer_h,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        self.scroll_to(scroll);
        self.invalidate();
    }

    /// Centre la fenêtre sur son écran (à sa première ouverture), ou la
    /// remet où elle était (`at`), si cet endroit est toujours sur un écran.
    pub(super) fn settle(&self, at: Option<(i32, i32)>) {
        let mut now = RECT::default();
        // SAFETY: la fenêtre est vivante ; les rectangles sont à nous.
        let (x, y) = unsafe {
            GetWindowRect(self.hwnd, &mut now);
            let (w, h) = (now.right - now.left, now.bottom - now.top);
            let wanted = at.map(|(x, y)| RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            });
            match wanted.filter(|wanted| !MonitorFromRect(wanted, MONITOR_DEFAULTTONULL).is_null())
            {
                Some(wanted) => (wanted.left, wanted.top),
                None => {
                    let work = work_of(MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST));
                    (
                        work.left + (work.right - work.left - w).max(0) / 2,
                        work.top + (work.bottom - work.top - h).max(0) / 2,
                    )
                }
            }
        };
        // SAFETY: la fenêtre est vivante, sur ce fil.
        unsafe {
            SetWindowPos(
                self.hwnd,
                std::ptr::null_mut(),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    /// Montre la fenêtre, devant les autres apps.
    pub(super) fn show(&self) {
        // Comme dans une boîte de dialogue : le cadre pointillé du clavier
        // n'apparaît qu'à la première touche (Tab, une flèche), sauf si
        // l'utilisateur a demandé à Windows de toujours le montrer.
        let mut always = 0i32;
        // SAFETY: `always` est à nous ; l'appel ne fait que l'écrire.
        let known = unsafe {
            SystemParametersInfoW(
                SPI_GETKEYBOARDCUES,
                0,
                (&raw mut always).cast::<c_void>(),
                0,
            )
        } != 0;
        let cues = if known && always != 0 {
            UIS_CLEAR
        } else {
            UIS_SET
        };
        let state = cues as usize | ((UISF_HIDEFOCUS | UISF_HIDEACCEL) as usize) << 16;
        // SAFETY: la fenêtre est vivante, sur ce fil.
        unsafe {
            SendMessageW(self.hwnd, WM_CHANGEUISTATE, state, 0);
            ShowWindow(
                self.hwnd,
                if IsIconic(self.hwnd) != 0 {
                    SW_RESTORE
                } else {
                    SW_SHOWNORMAL
                },
            );
            // Peinte tout de suite : jamais une fenêtre vide, même un
            // instant.
            UpdateWindow(self.hwnd);
            SetForegroundWindow(self.hwnd);
        }
    }

    /// La fenêtre est-elle à l'écran (ouverte, pas réduite) ?
    #[cfg(test)]
    pub(super) fn is_shown(&self) -> bool {
        // SAFETY: simples questions sur notre fenêtre.
        self.alive.get() && unsafe { IsWindowVisible(self.hwnd) != 0 && IsIconic(self.hwnd) == 0 }
    }

    /// Hauteur du contenu, en pixels.
    #[cfg(test)]
    pub(super) fn content_height(&self) -> i32 {
        self.content.get().1
    }

    pub(super) fn invalidate(&self) {
        // SAFETY: la fenêtre est vivante, sur ce fil ; tout est à repeindre.
        unsafe {
            RedrawWindow(
                self.hwnd,
                std::ptr::null(),
                std::ptr::null_mut(),
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
            );
        }
    }

    /// Une part de la fenêtre est à repeindre (place dans le contenu).
    pub(super) fn invalidate_part(&self, part: &RECT) {
        let scroll = self.scroll.get();
        let part = RECT {
            left: part.left,
            top: part.top - scroll,
            right: part.right,
            bottom: part.bottom - scroll,
        };
        // SAFETY: la fenêtre est vivante, sur ce fil ; `part` vit jusqu'au
        // retour.
        unsafe {
            RedrawWindow(self.hwnd, &part, std::ptr::null_mut(), RDW_INVALIDATE);
        }
    }

    // ——— Le défilement ———

    fn view_height(&self) -> i32 {
        let mut client = RECT::default();
        // SAFETY: la fenêtre est vivante ; `client` est à nous.
        unsafe { GetClientRect(self.hwnd, &mut client) };
        client.bottom - client.top
    }

    /// Fait défiler le contenu jusqu'à `wanted` pixels de son début.
    pub(super) fn scroll_to(&self, wanted: i32) {
        let old = self.scroll.get();
        let new = scroll_within(wanted, self.content.get().1, self.view_height());
        if new == old {
            return;
        }
        self.scroll.set(new);
        let info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_POS,
            nMin: 0,
            nMax: 0,
            nPage: 0,
            nPos: new,
            nTrackPos: 0,
        };
        // SAFETY: la fenêtre est vivante, sur ce fil ; ses contrôles suivent
        // (`SW_SCROLLCHILDREN`), ce qui se découvre est repeint.
        unsafe {
            ScrollWindowEx(
                self.hwnd,
                0,
                old - new,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                SW_SCROLLCHILDREN | SW_INVALIDATE | SW_ERASE,
            );
            SetScrollInfo(self.hwnd, SB_VERT, &info, 1);
            UpdateWindow(self.hwnd);
        }
    }

    /// La molette : `delta` vaut 120 par cran, vers le haut.
    fn wheel(&self, delta: i32) {
        self.scroll_to(self.scroll.get() - delta * self.px(64) / WHEEL_DELTA as i32);
    }

    /// Le contrôle qui a le clavier se voit en entier.
    fn reveal_focus(&self) {
        let mut rect = RECT::default();
        // SAFETY: lecture du contrôle qui a le clavier, s'il est à nous, et
        // de sa place dans notre fenêtre ; les rectangles sont à nous.
        let known = unsafe {
            let focus = GetFocus();
            if focus.is_null() || IsChild(self.hwnd, focus) == 0 {
                return;
            }
            let mut corner = POINT { x: 0, y: 0 };
            GetWindowRect(focus, &mut rect) != 0 && {
                corner.x = rect.left;
                corner.y = rect.top;
                let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
                ScreenToClient(self.hwnd, &mut corner);
                rect = RECT {
                    left: corner.x,
                    top: corner.y,
                    right: corner.x + w,
                    bottom: corner.y + h,
                };
                true
            }
        };
        if known {
            let by = scroll_to_reveal(rect.top, rect.bottom, self.view_height(), self.px(12));
            self.scroll_to(self.scroll.get() + by);
        }
    }

    // ——— L'apparence ———

    /// Le thème de Windows pour ce contrôle, selon l'apparence du panneau.
    fn theme(&self, hwnd: HWND) {
        let dark = self.paint().look == Look::Dark;
        let class = class_of(hwnd);
        let name = match (dark, class.as_str()) {
            (true, "ComboBox") => w!("DarkMode_CFD"),
            (true, "Button" | "msctls_trackbar32") => w!("DarkMode_Explorer"),
            _ => std::ptr::null(),
        };
        // SAFETY: le contrôle est vivant ; le nom est un texte constant, ou
        // rien (le thème ordinaire).
        unsafe { SetWindowTheme(hwnd, name, std::ptr::null()) };
    }

    /// Applique l'apparence voulue (celle de Windows, ou celle qu'on
    /// impose) à la fenêtre et à ses contrôles.
    fn apply_look(&self) {
        if self.applying.replace(true) {
            return;
        }
        let look = self.forced.get().unwrap_or_else(system_look);
        *self.paint.borrow_mut() = Rc::new(Paint::new(look));
        self.glyph.set(0);
        let dark = i32::from(look == Look::Dark);
        // SAFETY: la fenêtre est vivante, sur ce fil ; `dark` vit jusqu'au
        // retour. La barre de titre et les barres de défilement suivent
        // l'apparence ; le cadre est redessiné.
        unsafe {
            DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                (&raw const dark).cast::<c_void>(),
                size_of::<i32>() as u32,
            );
            SetWindowTheme(
                self.hwnd,
                if dark == 1 {
                    w!("DarkMode_Explorer")
                } else {
                    std::ptr::null()
                },
                std::ptr::null(),
            );
        }
        for child in children(self.hwnd) {
            self.theme(child);
        }
        // SAFETY: comme ci-dessus.
        unsafe {
            SetWindowPos(
                self.hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        self.applying.set(false);
        self.invalidate();
    }

    /// Les polices de cet écran, pour tous les contrôles.
    fn apply_fonts(&self) {
        *self.fonts.borrow_mut() = Rc::new(Fonts::new(self.dpi()));
        self.glyph.set(0);
        let tones = self.tones.borrow().clone();
        for child in children(self.hwnd) {
            let bold = tones.get(&(child as isize)).is_some_and(|tone| tone.bold);
            // SAFETY: message à un contrôle vivant de ce fil ; la police vit
            // autant que le panneau (ou jusqu'au prochain changement).
            unsafe { SendMessageW(child, WM_SETFONT, self.font(bold) as WPARAM, 0) };
        }
    }

    /// Prépare la fenêtre à être montrée : l'apparence de Windows.
    pub(super) fn dress(&self) {
        self.apply_look();
    }

    // ——— Le dessin ———

    /// Peint le fond, les cadres et les traits dans `clip` (de la fenêtre),
    /// puis ce que `extra` y ajoute (il reçoit le défilement).
    fn draw(&self, hdc: HDC, clip: &RECT, extra: &dyn Fn(HDC, &RECT, i32)) {
        let paint = self.paint();
        let scroll = self.scroll.get();
        let radius = self.px(8);
        let touches =
            |rect: &RECT| rect.bottom - scroll > clip.top && rect.top - scroll < clip.bottom;
        // SAFETY: `hdc` est un contexte de dessin valide le temps de
        // l'appel ; les pinceaux vivent autant que `paint`, gardé ici ; ce
        // qui était choisi dans le contexte y est remis.
        unsafe {
            FillRect(hdc, clip, paint.window);
            let old_pen = SelectObject(hdc, paint.edge as HGDIOBJ);
            let old_brush = SelectObject(hdc, paint.card as HGDIOBJ);
            for card in self.cards.borrow().iter().filter(|card| touches(card)) {
                RoundRect(
                    hdc,
                    card.left,
                    card.top - scroll,
                    card.right,
                    card.bottom - scroll,
                    radius,
                    radius,
                );
            }
            SelectObject(hdc, old_pen);
            SelectObject(hdc, old_brush);
            for line in self.lines.borrow().iter().filter(|line| touches(line)) {
                let line = RECT {
                    left: line.left,
                    top: line.top - scroll,
                    right: line.right,
                    bottom: line.bottom - scroll,
                };
                FillRect(hdc, &line, paint.line);
            }
        }
        extra(hdc, clip, scroll);
    }

    /// Comme [`Panel::draw`], mais d'un seul coup à l'écran (rien ne
    /// clignote).
    fn draw_buffered(&self, hdc: HDC, clip: &RECT, extra: &dyn Fn(HDC, &RECT, i32)) {
        let (width, height) = (clip.right - clip.left, clip.bottom - clip.top);
        if width <= 0 || height <= 0 {
            return;
        }
        // SAFETY: l'image de travail et son contexte sont créés et rendus
        // ici ; `hdc` est valide le temps de l'appel.
        unsafe {
            let memory = CreateCompatibleDC(hdc);
            let bitmap = CreateCompatibleBitmap(hdc, width, height);
            if memory.is_null() || bitmap.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap as HGDIOBJ);
                }
                if !memory.is_null() {
                    DeleteDC(memory);
                }
                self.draw(hdc, clip, extra);
                return;
            }
            let old = SelectObject(memory, bitmap as HGDIOBJ);
            SetViewportOrgEx(memory, -clip.left, -clip.top, std::ptr::null_mut());
            self.draw(memory, clip, extra);
            SetViewportOrgEx(memory, 0, 0, std::ptr::null_mut());
            BitBlt(
                hdc, clip.left, clip.top, width, height, memory, 0, 0, SRCCOPY,
            );
            SelectObject(memory, old);
            DeleteObject(bitmap as HGDIOBJ);
            DeleteDC(memory);
        }
    }

    /// Dessine une case à cocher : sa coche (celle du thème de Windows), et
    /// son libellé à nos couleurs.
    fn draw_check(&self, draw: &NMCUSTOMDRAW, label: &[u16]) -> LRESULT {
        if draw.dwDrawStage != CDDS_PREPAINT {
            return CDRF_DODEFAULT as LRESULT;
        }
        let (hwnd, hdc) = (draw.hdr.hwndFrom, draw.hdc);
        let paint = self.paint();
        let fonts = self.fonts();
        let glyph = self.glyph();
        let mut client = RECT::default();
        // SAFETY: `hwnd` est la case qui demande son dessin et `hdc` son
        // contexte, valides le temps de l'appel ; le thème est pris et rendu
        // ici ; ce qui était choisi dans le contexte y est remis.
        unsafe {
            GetClientRect(hwnd, &mut client);
            FillRect(hdc, &client, paint.card);
            let state = SendMessageW(hwnd, BM_GETSTATE, 0, 0) as u32;
            let checked = state & BST_CHECKED != 0;
            let pushed = state & BST_PUSHED != 0;
            let hot = state & BST_HOT != 0;
            let mut mark = RECT {
                left: 0,
                top: 1 + (fonts.line - glyph).max(0) / 2,
                right: glyph,
                bottom: 1 + (fonts.line - glyph).max(0) / 2 + glyph,
            };
            let theme = OpenThemeDataForDpi(hwnd, w!("Button"), self.dpi());
            if theme != 0 {
                let base = if checked {
                    CBS_CHECKEDNORMAL
                } else {
                    CBS_UNCHECKEDNORMAL
                };
                let part = base + if pushed { 2 } else { i32::from(hot) };
                DrawThemeBackground(theme, hdc, BP_CHECKBOX, part, &mark, std::ptr::null());
                CloseThemeData(theme);
            } else {
                let mut kind = DFCS_BUTTONCHECK;
                if checked {
                    kind |= DFCS_CHECKED;
                }
                if pushed {
                    kind |= DFCS_PUSHED;
                }
                DrawFrameControl(hdc, &mut mark, DFC_BUTTON, kind);
            }
            let old = SelectObject(hdc, fonts.text as HGDIOBJ);
            SetBkMode(hdc, TRANSPARENT as i32);
            SetTextColor(hdc, paint.colors.text);
            let mut text = RECT {
                left: glyph + self.px(8),
                top: 1,
                right: client.right - 1,
                bottom: client.bottom,
            };
            let format = DT_NOPREFIX | DT_WORDBREAK;
            let mut used = text;
            DrawTextW(
                hdc,
                label.as_ptr(),
                label.len() as i32,
                &mut used,
                format | DT_CALCRECT,
            );
            DrawTextW(hdc, label.as_ptr(), label.len() as i32, &mut text, format);
            // Le cadre pointillé du clavier, quand on s'en sert.
            let hidden = SendMessageW(hwnd, WM_QUERYUISTATE, 0, 0) as u32 & UISF_HIDEFOCUS != 0;
            if state & BST_FOCUS != 0 && !hidden {
                let around = RECT {
                    left: used.left - 2,
                    top: used.top - 1,
                    right: (used.right + 2).min(client.right),
                    bottom: (used.bottom + 1).min(client.bottom),
                };
                SetTextColor(hdc, rgb(0, 0, 0));
                SetBkColor(hdc, rgb(255, 255, 255));
                DrawFocusRect(hdc, &around);
            }
            SelectObject(hdc, old);
        }
        CDRF_SKIPDEFAULT as LRESULT
    }

    /// Dessine un lien : son libellé souligné, à la couleur des liens.
    fn draw_link(&self, item: &DRAWITEMSTRUCT, label: &[u16]) {
        let paint = self.paint();
        let fonts = self.fonts();
        let mut place = item.rcItem;
        // SAFETY: `item.hDC` est le contexte de dessin du lien, valide le
        // temps de l'appel ; ce qui y était choisi y est remis.
        unsafe {
            FillRect(item.hDC, &place, paint.window);
            let old = SelectObject(item.hDC, fonts.link as HGDIOBJ);
            SetBkMode(item.hDC, TRANSPARENT as i32);
            SetTextColor(item.hDC, paint.colors.link);
            DrawTextW(
                item.hDC,
                label.as_ptr(),
                label.len() as i32,
                &mut place,
                DT_NOPREFIX | DT_SINGLELINE | DT_CENTER | DT_VCENTER,
            );
            if item.itemState & ODS_FOCUS != 0 && item.itemState & ODS_NOFOCUSRECT == 0 {
                SetTextColor(item.hDC, rgb(0, 0, 0));
                SetBkColor(item.hDC, rgb(255, 255, 255));
                DrawFocusRect(item.hDC, &item.rcItem);
            }
            SelectObject(item.hDC, old);
        }
    }

    // ——— Les messages ———

    /// Ce que le panneau fait d'un message de sa fenêtre. `extra` peint ce
    /// que la fenêtre a de plus que ses cadres (les aperçus de la galerie).
    pub(super) fn handle(
        &self,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        extra: &dyn Fn(HDC, &RECT, i32),
    ) -> Handled {
        match message {
            WM_ERASEBKGND => Handled::Done(1),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                // SAFETY: la fenêtre est vivante, sur ce fil ; `paint` est à
                // nous ; le contexte est rendu par `EndPaint`.
                unsafe {
                    let hdc = BeginPaint(self.hwnd, &mut paint);
                    self.draw_buffered(hdc, &paint.rcPaint, extra);
                    EndPaint(self.hwnd, &paint);
                }
                Handled::Done(0)
            }
            WM_PRINTCLIENT => {
                let mut client = RECT::default();
                // SAFETY: la fenêtre est vivante ; `client` est à nous.
                unsafe { GetClientRect(self.hwnd, &mut client) };
                self.draw(wparam as HDC, &client, extra);
                Handled::Done(0)
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                let paint = self.paint();
                let tone = self
                    .tones
                    .borrow()
                    .get(&lparam)
                    .copied()
                    .unwrap_or(Tone::TEXT);
                let (ground, brush) = if tone.on_card {
                    (paint.colors.card, paint.card)
                } else {
                    (paint.colors.window, paint.window)
                };
                // SAFETY: `wparam` est le contexte de dessin du contrôle,
                // valide le temps de l'appel.
                unsafe {
                    SetTextColor(
                        wparam as HDC,
                        if tone.dim {
                            paint.colors.dim
                        } else {
                            paint.colors.text
                        },
                    );
                    SetBkColor(wparam as HDC, ground);
                }
                Handled::Done(brush as LRESULT)
            }
            WM_CTLCOLORLISTBOX if self.paint().look == Look::Dark => {
                let paint = self.paint();
                // SAFETY: comme ci-dessus (la liste ouverte d'un choix).
                unsafe {
                    SetTextColor(wparam as HDC, paint.colors.text);
                    SetBkColor(wparam as HDC, paint.colors.card);
                }
                Handled::Done(paint.card as LRESULT)
            }
            WM_NOTIFY => {
                // SAFETY: pour ce message, `lparam` pointe vers l'en-tête
                // d'une notification, valide le temps de l'appel ; celle du
                // dessin d'un contrôle commence par cet en-tête.
                let header = unsafe { &*(lparam as *const NMHDR) };
                if header.code != NM_CUSTOMDRAW {
                    return Handled::No;
                }
                let label = self
                    .checks
                    .borrow()
                    .get(&(header.hwndFrom as isize))
                    .cloned();
                match label {
                    Some(label) => {
                        // SAFETY: voir ci-dessus.
                        let draw = unsafe { &*(lparam as *const NMCUSTOMDRAW) };
                        Handled::Done(self.draw_check(draw, &label))
                    }
                    None => Handled::No,
                }
            }
            WM_DRAWITEM => {
                // SAFETY: pour ce message, `lparam` pointe vers ce qu'un
                // contrôle demande de dessiner, valide le temps de l'appel.
                let item = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
                let label = self.links.borrow().get(&(item.hwndItem as isize)).cloned();
                match label {
                    Some(label) => {
                        self.draw_link(item, &label);
                        Handled::Done(1)
                    }
                    None => Handled::No,
                }
            }
            // Sur un lien, le curseur est une main.
            WM_SETCURSOR if self.links.borrow().contains_key(&(wparam as isize)) => {
                // SAFETY: un curseur du système, partagé, par son numéro.
                unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_HAND)) };
                Handled::Done(1)
            }
            WM_VSCROLL => {
                let mut info = SCROLLINFO {
                    cbSize: size_of::<SCROLLINFO>() as u32,
                    fMask: SIF_ALL,
                    ..SCROLLINFO::default()
                };
                // SAFETY: la fenêtre est vivante ; `info` porte sa taille.
                unsafe { GetScrollInfo(self.hwnd, SB_VERT, &mut info) };
                let (now, page) = (self.scroll.get(), info.nPage as i32);
                let step = self.px(48);
                let wanted = match (wparam & 0xFFFF) as i32 {
                    SB_LINEUP => now - step,
                    SB_LINEDOWN => now + step,
                    SB_PAGEUP => now - (page - step).max(step),
                    SB_PAGEDOWN => now + (page - step).max(step),
                    SB_THUMBTRACK | SB_THUMBPOSITION => info.nTrackPos,
                    SB_TOP => 0,
                    SB_BOTTOM => i32::MAX / 2,
                    _ => now,
                };
                self.scroll_to(wanted);
                Handled::Done(0)
            }
            WM_MOUSEWHEEL => {
                self.wheel(i32::from((wparam >> 16) as u16 as i16));
                Handled::Done(0)
            }
            WM_ACTIVATE => {
                if (wparam & 0xFFFF) as u32 == WA_INACTIVE {
                    // SAFETY: lecture du contrôle qui a le clavier ; un
                    // message sans pointeur à notre fenêtre.
                    unsafe {
                        let focus = GetFocus();
                        if !focus.is_null() && IsChild(self.hwnd, focus) != 0 {
                            self.focus.set(focus as isize);
                        }
                        PostMessageW(self.hwnd, WM_APP_LOWER, 0, 0);
                    }
                } else {
                    // Au premier plan : devant les animaux, qui sont devant
                    // toutes les autres fenêtres.
                    // SAFETY: la fenêtre est vivante, sur ce fil.
                    unsafe {
                        SetWindowPos(
                            self.hwnd,
                            HWND_TOPMOST,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
                    self.focus_child();
                }
                Handled::Done(0)
            }
            WM_APP_LOWER => {
                // Derrière la fenêtre qui a pris le premier plan (et donc
                // derrière les animaux), comme toute fenêtre ordinaire.
                // SAFETY: la fenêtre est vivante, sur ce fil ; celle du
                // premier plan est seulement nommée.
                unsafe {
                    let front = GetForegroundWindow();
                    if front != self.hwnd {
                        SetWindowPos(
                            self.hwnd,
                            if front.is_null() {
                                HWND_NOTOPMOST
                            } else {
                                front
                            },
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
                }
                Handled::Done(0)
            }
            WM_SETFOCUS => {
                self.focus_child();
                Handled::Done(0)
            }
            WM_COMMAND if (wparam & 0xFFFF) as i32 == IDCANCEL && lparam == 0 => {
                // Échap : la fenêtre se ferme.
                // SAFETY: un message sans pointeur à notre fenêtre.
                unsafe { PostMessageW(self.hwnd, WM_CLOSE, 0, 0) };
                Handled::Done(0)
            }
            WM_GETMINMAXINFO if self.unbounded.get() => {
                // SAFETY: pour ce message, `lparam` pointe vers les limites
                // de taille de la fenêtre, à nous le temps de l'appel.
                let limits = unsafe { &mut *(lparam as *mut MINMAXINFO) };
                limits.ptMaxTrackSize.y = 30_000;
                limits.ptMaxSize.y = 30_000;
                Handled::Done(0)
            }
            WM_DPICHANGED => {
                self.dpi.set(((wparam & 0xFFFF) as u32).max(48));
                self.apply_fonts();
                // SAFETY: pour ce message, `lparam` pointe vers la place que
                // Windows propose, valide le temps de l'appel.
                let place = unsafe { *(lparam as *const RECT) };
                Handled::Relayout(Some(place))
            }
            WM_SETTINGCHANGE | WM_THEMECHANGED | WM_SYSCOLORCHANGE if !self.applying.get() => {
                self.apply_fonts();
                self.apply_look();
                Handled::Relayout(None)
            }
            WM_NCDESTROY => {
                self.alive.set(false);
                Handled::No
            }
            _ => Handled::No,
        }
    }

    /// Le clavier va au contrôle qui l'avait, ou au premier.
    fn focus_child(&self) {
        let remembered = self.focus.get() as HWND;
        // SAFETY: la fenêtre est vivante, sur ce fil ; le contrôle retenu
        // n'est repris que s'il est toujours le sien.
        unsafe {
            let child = if !remembered.is_null() && IsChild(self.hwnd, remembered) != 0 {
                remembered
            } else {
                GetNextDlgTabItem(self.hwnd, std::ptr::null_mut(), 0)
            };
            if !child.is_null() {
                SetFocus(child);
            }
        }
    }

    // ——— L'image de la fenêtre ———

    /// Enregistre en PNG l'image de la fenêtre entière (tout son contenu,
    /// même ce qui défilerait), en clair ou en sombre : la vitrine de la CI.
    pub(super) fn snapshot(&self, path: &Path, dark: bool) -> bool {
        self.forced
            .set(Some(if dark { Look::Dark } else { Look::Light }));
        self.unbounded.set(true);
        self.apply_look();
        let (width, height) = self.content.get();
        self.fit(width, height);
        let saved = self.capture(path);
        self.forced.set(None);
        self.unbounded.set(false);
        self.apply_look();
        self.fit(width, height);
        saved
    }

    fn capture(&self, path: &Path) -> bool {
        let mut window = RECT::default();
        let mut visible = RECT::default();
        // SAFETY: la fenêtre est vivante, sur ce fil ; les rectangles sont
        // à nous ; l'image de travail et son contexte sont créés et rendus
        // ici, et ses pixels ne sont lus que tant qu'elle existe.
        let (pixels, width, height) = unsafe {
            RedrawWindow(
                self.hwnd,
                std::ptr::null(),
                std::ptr::null_mut(),
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_FRAME | RDW_UPDATENOW,
            );
            // Le compositeur de Windows a le temps de refaire son image.
            DwmFlush();
            DwmFlush();
            GetWindowRect(self.hwnd, &mut window);
            let (width, height) = (window.right - window.left, window.bottom - window.top);
            if width <= 0 || height <= 0 || width > 16_384 || height > 32_768 {
                return false;
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    // Négative : les lignes vont de haut en bas.
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..BITMAPINFOHEADER::default()
                },
                ..BITMAPINFO::default()
            };
            let screen = GetDC(std::ptr::null_mut());
            let memory = CreateCompatibleDC(screen);
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                screen,
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            );
            ReleaseDC(std::ptr::null_mut(), screen);
            if memory.is_null() || bitmap.is_null() || bits.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap as HGDIOBJ);
                }
                if !memory.is_null() {
                    DeleteDC(memory);
                }
                return false;
            }
            let old = SelectObject(memory, bitmap as HGDIOBJ);
            let count = (width * height * 4) as usize;
            let read = || std::slice::from_raw_parts(bits.cast::<u8>(), count).to_vec();
            // D'abord telle que le compositeur la montre (avec sa barre de
            // titre d'aujourd'hui) ; sinon telle qu'elle se dessine.
            let mut pixels = Vec::new();
            if PrintWindow(self.hwnd, memory, PW_RENDERFULLCONTENT) != 0 {
                GdiFlush();
                pixels = read();
            }
            if pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0)
            {
                PrintWindow(self.hwnd, memory, 0);
                GdiFlush();
                pixels = read();
            }
            SelectObject(memory, old);
            DeleteObject(bitmap as HGDIOBJ);
            DeleteDC(memory);
            // Le bord visible de la fenêtre (le reste est son ombre).
            let known = DwmGetWindowAttribute(
                self.hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&raw mut visible).cast::<c_void>(),
                size_of::<RECT>() as u32,
            ) == 0;
            if !known || visible.right <= visible.left {
                visible = window;
            }
            (pixels, width, height)
        };
        let crop = [
            (visible.left - window.left).clamp(0, width - 1),
            (visible.top - window.top).clamp(0, height - 1),
            (visible.right - window.left).clamp(1, width),
            (visible.bottom - window.top).clamp(1, height),
        ];
        let (out_w, out_h) = ((crop[2] - crop[0]).max(1), (crop[3] - crop[1]).max(1));
        let mut rgba = Vec::with_capacity((out_w * out_h * 4) as usize);
        for y in crop[1]..crop[1] + out_h {
            let row = (y * width * 4) as usize;
            for x in crop[0]..crop[0] + out_w {
                let at = row + (x * 4) as usize;
                let Some(pixel) = pixels.get(at..at + 4) else {
                    return false;
                };
                rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
        }
        write_png(path, out_w as u32, out_h as u32, &rgba)
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        if self.alive.replace(false) {
            // SAFETY: la fenêtre est à nous, sur ce fil, et vit encore.
            unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

/// Écrit une image RGBA en PNG.
fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> bool {
    let Ok(file) = std::fs::File::create(path) else {
        return false;
    };
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(rgba))
        .is_ok()
}

/// Le clavier et la molette dans les fenêtres de Mouflon. Windows appelle
/// ceci pour chaque message que la boucle de l'app retire de sa file, avant
/// qu'elle le distribue : si le message est pour un panneau, le clavier y
/// fait ce qu'il fait dans toute boîte de dialogue (Tab et Maj+Tab d'un
/// contrôle à l'autre, les flèches, Entrée sur un bouton, Échap pour
/// fermer), et la molette fait défiler la fenêtre au lieu de changer un
/// réglage au passage. Un message traité ici est vidé : la boucle n'en fait
/// plus rien. (C'est la manière prévue par Windows pour une fenêtre dont on
/// ne tient pas la boucle de messages : celle de l'app est à winit.)
unsafe extern "system" fn on_message(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // (1 : le message est retiré de la file, `PM_REMOVE`.)
    if code >= 0 && wparam == 1 && lparam != 0 {
        // SAFETY: pour ce crochet, `lparam` pointe vers le message que la
        // boucle vient de lire, à nous le temps de l'appel.
        let msg = unsafe { &mut *(lparam as *mut MSG) };
        if treated(msg) {
            msg.message = WM_NULL;
            msg.wParam = 0;
            msg.lParam = 0;
        }
    }
    // SAFETY: les arguments sont ceux que Windows vient de nous donner (le
    // crochet suivant, s'il y en a un, les reçoit tels quels).
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// Ce message est-il pour un panneau, qui l'a traité ?
fn treated(msg: &MSG) -> bool {
    let keyboard = (WM_KEYFIRST..=WM_KEYLAST).contains(&msg.message);
    if (!keyboard && msg.message != WM_MOUSEWHEEL) || msg.hwnd.is_null() {
        return false;
    }
    // SAFETY: simple question sur une fenêtre nommée par le message.
    let root = unsafe { GetAncestor(msg.hwnd, GA_ROOT) };
    let Some(panel) = panel_of(root) else {
        return false;
    };
    if msg.message == WM_MOUSEWHEEL {
        // La liste ouverte d'un choix défile d'elle-même.
        let open = class_of(msg.hwnd) == "ComboBox"
            // SAFETY: message sans pointeur à un contrôle de notre fenêtre.
            && unsafe { SendMessageW(msg.hwnd, CB_GETDROPPEDSTATE, 0, 0) } != 0;
        if open {
            return false;
        }
        panel.wheel(i32::from((msg.wParam >> 16) as u16 as i16));
        return true;
    }
    // SAFETY: `root` est notre fenêtre, sur ce fil ; `msg` est valide.
    let handled = unsafe { IsDialogMessageW(root, msg) } != 0;
    if handled && msg.message == WM_KEYDOWN {
        panel.reveal_focus();
    }
    handled
}

/// Pose le crochet des messages du fil principal (une fois).
fn watch_messages() {
    thread_local! {
        static WATCHING: Cell<bool> = const { Cell::new(false) };
    }
    if WATCHING.replace(true) {
        return;
    }
    // SAFETY: un crochet pour les seuls messages de ce fil (`WH_GETMESSAGE`),
    // dont la procédure vit autant que le programme ; il n'est jamais
    // retiré (le fil principal dure autant que le processus).
    let hook = unsafe {
        SetWindowsHookExW(
            WH_GETMESSAGE,
            Some(on_message),
            std::ptr::null_mut(),
            GetCurrentThreadId(),
        )
    };
    if hook.is_null() {
        log::warn!("clavier des fenêtres de Mouflon : crochet refusé par Windows");
    }
}

/// Pour les tests des fenêtres : un message sans pointeur à une fenêtre ou
/// à un contrôle de ce fil, comme Windows ou l'utilisateur en envoient.
#[cfg(test)]
pub(super) fn send(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: la fenêtre appartient à ce fil ; ni `wparam` ni `lparam` ne
    // sont des pointeurs dans les messages que les tests envoient.
    unsafe { SendMessageW(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La luminance d'une couleur (WCAG), de 0 à 1.
    fn luminance(color: COLORREF) -> f64 {
        let channel = |shift: u32| {
            let c = f64::from((color >> shift) & 0xFF) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(8) + 0.0722 * channel(16)
    }

    fn contrast(a: COLORREF, b: COLORREF) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn texts_stay_readable_on_their_grounds_in_light_and_in_dark() {
        for look in [Look::Light, Look::Dark] {
            let c = look.colors();
            for ground in [c.window, c.card] {
                // Le seuil des textes ordinaires (WCAG AA) : 4,5.
                assert!(contrast(c.text, ground) >= 7.0, "{look:?} : texte");
                assert!(contrast(c.dim, ground) >= 4.5, "{look:?} : explication");
                assert!(contrast(c.link, ground) >= 4.5, "{look:?} : lien");
            }
            // Un cadre se distingue du fond par son bord.
            assert_ne!(c.edge, c.card);
            assert_ne!(c.window, c.card);
        }
    }

    #[test]
    fn scrolling_stops_at_both_ends_of_the_content() {
        assert_eq!(scroll_within(-40, 1000, 400), 0);
        assert_eq!(scroll_within(250, 1000, 400), 250);
        assert_eq!(scroll_within(900, 1000, 400), 600);
        // Un contenu qui tient dans la fenêtre ne défile pas.
        assert_eq!(scroll_within(120, 300, 400), 0);
    }

    #[test]
    fn a_control_that_takes_the_keyboard_scrolls_into_view() {
        // Déjà visible : rien.
        assert_eq!(scroll_to_reveal(100, 130, 400, 12), 0);
        // Coupé par le bas : on descend juste assez.
        assert_eq!(scroll_to_reveal(380, 420, 400, 12), 32);
        // Sorti par le haut : on remonte.
        assert_eq!(scroll_to_reveal(-30, 0, 400, 12), -42);
        // Plus haut que la vue : son haut d'abord.
        assert_eq!(scroll_to_reveal(50, 600, 400, 12), 38);
    }

    #[test]
    fn an_ampersand_in_a_label_is_shown_as_it_is() {
        assert_eq!(escaped("Sons & musique"), "Sons && musique");
        assert_eq!(escaped("Sons"), "Sons");
    }
}
