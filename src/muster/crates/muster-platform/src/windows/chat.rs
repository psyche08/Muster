//! Translate Chromium's Windows UI Automation tree into the shared chat tree.
use super::{Windows, provider};
use crate::Platform;
use muster_agent::{
    Error,
    desktop_chat::{AxNode, AxTree, ChatApp},
};
pub(crate) use provider::{observe_process_start, process, process_start};
use std::cell::OnceCell;
use std::time::{Duration, Instant};
use windows::{
    Win32::{Foundation::HWND, System::Com::*, UI::Accessibility::*},
    core::{AgileReference, BSTR, Result as WinResult},
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

thread_local! { static CLIENT: OnceCell<IUIAutomation> = const { OnceCell::new() }; }
fn client() -> WinResult<IUIAutomation> {
    CLIENT.with(|cell| {
        if let Some(c) = cell.get() {
            return Ok(c.clone());
        }
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let c: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
            let _ = cell.set(c.clone());
            Ok(c)
        }
    })
}

#[derive(Clone, Debug)]
pub(crate) struct Element {
    pid: u32,
    hwnd: isize,
    node: Option<AgileReference<IUIAutomationElement>>,
}
impl PartialEq for Element {
    fn eq(&self, other: &Self) -> bool {
        self.pid == other.pid && self.hwnd == other.hwnd && self.node == other.node
    }
}
fn unavailable() -> windows::core::Error {
    windows::core::Error::from_hresult(windows::core::HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32))
}
impl Element {
    pub(crate) fn application(pid: u32) -> Self {
        Self {
            pid,
            hwnd: provider::window(pid).unwrap_or(0),
            node: None,
        }
    }
    pub(crate) fn enable_manual_accessibility(&self) {} // UIA activates the Chromium provider on first request.
    fn control(&self) -> WinResult<IUIAutomationElement> {
        client()?;
        if self.hwnd == 0
            || provider::window(self.pid) != Some(self.hwnd)
            || console_ready().is_err()
        {
            return Err(unavailable());
        }
        self.node.as_ref().ok_or_else(unavailable)?.resolve()
    }
    pub(crate) fn press(&self) -> WinResult<()> {
        let node = self.control()?;
        unsafe {
            if !node.CurrentIsEnabled()?.as_bool() {
                return Err(unavailable());
            }
            if let Ok(pattern) =
                node.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
            {
                return pattern.Invoke();
            }
            if let Ok(pattern) = node.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                UIA_SelectionItemPatternId,
            ) {
                return pattern.Select();
            }
            if let Ok(pattern) = node.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(
                UIA_ExpandCollapsePatternId,
            ) {
                return pattern.Expand();
            }
            if let Ok(toggle) =
                node.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
            {
                return toggle.Toggle();
            }
            node.GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(
                UIA_LegacyIAccessiblePatternId,
            )?
            .DoDefaultAction()
        }
    }
    pub(crate) fn set_bool(&self, attr: &str, value: bool) -> WinResult<()> {
        if attr != "AXFocused" || !value {
            return Err(unavailable());
        }
        unsafe { self.control()?.SetFocus() }
    }
    pub(crate) fn has_action(&self, action: &str) -> bool {
        let Ok(n) = self.control() else { return false };
        unsafe {
            match action {
                "AXScrollDownByPage" => n
                    .GetCurrentPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId)
                    .is_ok(),
                "AXScrollToVisible" => n
                    .GetCurrentPatternAs::<IUIAutomationScrollItemPattern>(UIA_ScrollItemPatternId)
                    .is_ok(),
                _ => false,
            }
        }
    }
    pub(crate) fn action_names(&self) -> Vec<&'static str> {
        ["AXScrollDownByPage", "AXScrollToVisible"]
            .into_iter()
            .filter(|a| self.has_action(a))
            .collect()
    }
    pub(crate) fn perform(&self, action: &str) -> WinResult<()> {
        let n = self.control()?;
        unsafe {
            match action {
                "AXScrollDownByPage" => n
                    .GetCurrentPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId)?
                    .Scroll(ScrollAmount_NoAmount, ScrollAmount_LargeIncrement),
                "AXScrollToVisible" => n
                    .GetCurrentPatternAs::<IUIAutomationScrollItemPattern>(UIA_ScrollItemPatternId)?
                    .ScrollIntoView(),
                _ => Err(unavailable()),
            }
        }
    }
}
pub(crate) fn draft(_: ChatApp, e: &Element, text: &str) -> WinResult<()> {
    let n = e.control()?;
    unsafe {
        if n.CurrentControlType()? != UIA_EditControlTypeId || !n.CurrentIsEnabled()?.as_bool() {
            return Err(unavailable());
        }
        if let Ok(p) = n.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
            return p.SetValue(&BSTR::from(text));
        }
        n.GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(
            UIA_LegacyIAccessiblePatternId,
        )?
        .SetValue(&BSTR::from(text))
    }
}
pub(crate) fn target_input_generation(_: u32) -> u64 {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if unsafe { GetLastInputInfo(&mut info) } != 0 {
        return u64::from(info.dwTime);
    }
    // A failed observation must never produce two matching generation reads.
    static FAILED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1 << 32);
    FAILED.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
pub(crate) fn console_ready() -> Result<(), Error> {
    if Windows.locked() {
        Err(Error::Unavailable(
            "Windows desktop session is locked or disconnected".into(),
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn chat_window(pid: u32) -> Result<Element, Error> {
    window(&Element::application(pid))
}
pub(crate) fn window(e: &Element) -> Result<Element, Error> {
    if e.hwnd == 0 || provider::window(e.pid) != Some(e.hwnd) {
        return Err(Error::Conflict(
            "Provider window changed or is ambiguous".into(),
        ));
    }
    Ok(Element {
        node: None,
        ..e.clone()
    })
}
#[allow(non_upper_case_globals)] // SDK constants retain their Win32 names.
fn role(control: UIA_CONTROLTYPE_ID, aria: &str, properties: &str) -> (&'static str, &'static str) {
    match aria.split_ascii_whitespace().next().unwrap_or("") {
        "navigation" => return ("AXGroup", "AXLandmarkNavigation"),
        "main" => return ("AXGroup", "AXLandmarkMain"),
        "complementary" => return ("AXGroup", "AXLandmarkComplementary"),
        "dialog" | "alertdialog" => return ("AXGroup", "AXApplicationDialog"),
        "article" => return ("AXGroup", "AXDocumentArticle"),
        "heading" => return ("AXHeading", ""),
        "radiogroup" => return ("AXRadioGroup", ""),
        "radio" => return ("AXRadioButton", ""),
        "textbox" => return ("AXTextArea", ""),
        "menuitemradio" => return ("AXMenuItem", "AXMenuItemRadio"),
        "menuitemcheckbox" => return ("AXMenuItem", "AXMenuItemCheckBox"),
        _ => {}
    }
    let r = match control {
        UIA_DocumentControlTypeId => "AXWebArea",
        UIA_WindowControlTypeId => "AXWindow",
        UIA_ButtonControlTypeId
            if properties
                .split(';')
                .any(|v| matches!(v, "haspopup=menu" | "haspopup=true")) =>
        {
            "AXPopUpButton"
        }
        UIA_ButtonControlTypeId => "AXButton",
        UIA_ComboBoxControlTypeId => "AXPopUpButton",
        UIA_EditControlTypeId => "AXTextArea",
        UIA_HyperlinkControlTypeId => "AXLink",
        UIA_TextControlTypeId => "AXStaticText",
        UIA_ListControlTypeId => "AXList",
        UIA_ListItemControlTypeId => "AXRow",
        UIA_RadioButtonControlTypeId => "AXRadioButton",
        UIA_CheckBoxControlTypeId => "AXCheckBox",
        UIA_ToolBarControlTypeId => "AXToolbar",
        UIA_MenuControlTypeId => "AXMenu",
        UIA_MenuBarControlTypeId => "AXMenuBar",
        UIA_MenuItemControlTypeId => "AXMenuItem",
        UIA_ImageControlTypeId => "AXImage",
        UIA_TabItemControlTypeId => "AXRadioButton",
        UIA_ScrollBarControlTypeId => "AXScrollBar",
        _ => "AXGroup",
    };
    (r, "")
}
fn cached_string(n: &IUIAutomationElement, id: UIA_PROPERTY_ID) -> String {
    unsafe { n.GetCachedPropertyValue(id) }
        .ok()
        .and_then(|v| BSTR::try_from(&v).ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}
fn read(root: &Element) -> WinResult<(AxTree, Vec<Element>)> {
    let c = client()?;
    let started = Instant::now();
    unsafe {
        let cache = c.CreateCacheRequest()?;
        cache.SetTreeScope(TreeScope_Subtree)?;
        cache.SetTreeFilter(&c.RawViewCondition()?)?;
        for prop in [
            UIA_ControlTypePropertyId,
            UIA_AriaRolePropertyId,
            UIA_AriaPropertiesPropertyId,
            UIA_NamePropertyId,
            UIA_HelpTextPropertyId,
            UIA_AutomationIdPropertyId,
            UIA_IsEnabledPropertyId,
            UIA_ValueValuePropertyId,
            UIA_LegacyIAccessibleValuePropertyId,
            UIA_SelectionItemIsSelectedPropertyId,
            UIA_ToggleToggleStatePropertyId,
            UIA_LandmarkTypePropertyId,
        ] {
            cache.AddProperty(prop)?;
        }
        let top = c.ElementFromHandleBuildCache(HWND(root.hwnd as _), &cache)?;
        let mut tree = AxTree {
            complete: true,
            ..Default::default()
        };
        let mut elements = Vec::new();
        let mut stack = vec![(top, None::<usize>)];
        while let Some((n, parent)) = stack.pop() {
            if tree.nodes.len() >= 12000 || started.elapsed() > Duration::from_secs(4) {
                return Err(unavailable());
            }
            let properties = n.CachedAriaProperties()?.to_string();
            let (role, mut subrole) = role(
                n.CachedControlType()?,
                &n.CachedAriaRole()?.to_string(),
                &properties,
            );
            if subrole.is_empty() {
                let landmark = n.GetCachedPropertyValue(UIA_LandmarkTypePropertyId)?;
                subrole = match i32::try_from(&landmark).unwrap_or(0) {
                    80002 => "AXLandmarkMain",
                    80003 => "AXLandmarkNavigation",
                    _ => "",
                };
            }
            let title = n.CachedName()?.to_string();
            let mut value = cached_string(&n, UIA_ValueValuePropertyId);
            if value.is_empty() {
                value = cached_string(&n, UIA_LegacyIAccessibleValuePropertyId);
            }
            let url = if matches!(role, "AXLink" | "AXWebArea") {
                value.clone()
            } else {
                String::new()
            };
            if role == "AXStaticText" && value.is_empty() {
                value = title.clone();
            }
            let selected =
                bool::try_from(&n.GetCachedPropertyValue(UIA_SelectionItemIsSelectedPropertyId)?)
                    .unwrap_or(false)
                    || i32::try_from(&n.GetCachedPropertyValue(UIA_ToggleToggleStatePropertyId)?)
                        .ok()
                        == Some(ToggleState_On.0)
                    || properties.split(';').any(|v| {
                        matches!(
                            v,
                            "selected=true"
                                | "pressed=true"
                                | "checked=true"
                                | "current=page"
                                | "current=true"
                        )
                    });
            let i = tree.nodes.len();
            tree.nodes.push(AxNode {
                role: role.into(),
                subrole: subrole.into(),
                title,
                description: n.CachedHelpText()?.to_string(),
                value,
                url,
                identifier: n.CachedAutomationId()?.to_string(),
                placeholder: properties
                    .split(';')
                    .find_map(|v| v.strip_prefix("placeholder="))
                    .unwrap_or_default()
                    .into(),
                enabled: n.CachedIsEnabled()?.as_bool(),
                selected,
                parent,
                ..Default::default()
            });
            elements.push(Element {
                node: Some(AgileReference::new(&n)?),
                ..root.clone()
            });
            if let Some(p) = parent {
                tree.nodes[p].children.push(i);
            }
            // GetCachedChildren returns S_OK and a null array for a leaf.
            if let Ok(children) = n.GetCachedChildren() {
                for j in (0..children.Length()?).rev() {
                    stack.push((children.GetElement(j)?, Some(i)));
                }
            }
        }
        Ok((tree, elements))
    }
}
pub(crate) fn snapshot(root: &Element, _: bool) -> (AxTree, Vec<Element>) {
    read(root).unwrap_or_default()
}
