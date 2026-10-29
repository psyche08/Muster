//! Accessibility primitives: a bounded walk of an application's element
//! tree and the few attribute reads and writes the controller needs. AX is
//! IPC to the target process against its semantic controls, so it works
//! without screen pixels or keyboard focus, which is what lets a locked
//! Mac still take input (§3.6.2, pending the P0 device check).

use std::ffi::c_void;
use std::time::{Duration, Instant};

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, CFTypeID, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use core_foundation::url::CFURL;
use core_foundation_sys::array::CFArrayRef;
use core_foundation_sys::base::CFTypeRef;
use core_foundation_sys::string::CFStringRef;

pub type AXError = i32;
/// kAXErrorInvalidUIElement: the element is gone and answers nothing.
pub const INVALID_ELEMENT: AXError = -25202;

/// What a walk does after visiting a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visit {
    /// Visit the node's children too.
    Descend,
    /// Leave the node's children out; the walk goes on elsewhere.
    Skip,
    /// End the walk here.
    Stop,
}
type AXUIElementRef = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(
        options: core_foundation_sys::dictionary::CFDictionaryRef,
    ) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: CFStringRef,
        settable: *mut u8,
    ) -> AXError;
    fn AXUIElementCopyMultipleAttributeValues(
        element: AXUIElementRef,
        attributes: CFArrayRef,
        options: u32,
        values: *mut CFArrayRef,
    ) -> AXError;
    fn AXUIElementCopyActionNames(element: AXUIElementRef, names: *mut CFArrayRef) -> AXError;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> AXError;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout: f32) -> AXError;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXValueGetTypeID() -> CFTypeID;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, result: *mut c_void) -> bool;
    fn AXValueCreate(kind: u32, value: *const c_void) -> CFTypeRef;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        parameter: CFTypeRef,
        result: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementCopyElementAtPosition(
        application: AXUIElementRef,
        x: f32,
        y: f32,
        result: *mut AXUIElementRef,
    ) -> AXError;
}

pub const MAX_NODES: usize = 4096;
pub const MAX_CHILDREN: usize = 4096;
pub const WALK_BUDGET: Duration = Duration::from_secs(2);
const TIMEOUT: f32 = 0.25;

pub fn trusted() -> bool {
    // SAFETY: plain C call.
    unsafe { AXIsProcessTrusted() }
}

/// Asks for Accessibility with the system prompt (once per denial; the user
/// finishes in System Settings › Privacy & Security › Accessibility).
pub fn request_trust() -> bool {
    // SAFETY: the key is a CFString constant owned by the framework.
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
    // SAFETY: a valid dictionary reference for the duration of the call.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

/// One accessibility element (a retained CF object).
#[derive(Clone)]
pub struct Element(CFType);

// SAFETY: AXUIElementRef is a Core Foundation object; retain/release and the
// AX calls are thread-safe, and the controller serialises use behind mutexes.
unsafe impl Send for Element {}
unsafe impl Sync for Element {}

impl PartialEq for Element {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Element {
    fn bounded(value: CFType) -> Self {
        let element = Self(value);
        // AXUIElementSetMessagingTimeout applies only to this exact proxy,
        // not its children or equal proxies (macOS AXUIElement.h). Every
        // newly received element needs its own per-message IPC bound.
        element.set_timeout(TIMEOUT);
        element
    }

    pub fn application(pid: u32) -> Element {
        // SAFETY: returns a +1 reference.
        let raw = unsafe { AXUIElementCreateApplication(pid as i32) };
        // SAFETY: raw is a valid +1 CF object.
        Self::bounded(unsafe { CFType::wrap_under_create_rule(raw as CFTypeRef) })
    }

    /// Route a single Tab to this application's focused editor. Never Return.
    pub fn tab(&self) -> Result<(), AXError> {
        self.keyboard_pair(48)
    }

    /// Activate a button only after the caller verifies exact application focus.
    /// Application-directed input works without global pointer or foreground routing.
    pub fn space(&self) -> Result<(), AXError> {
        self.keyboard_pair(49)
    }

    fn keyboard_pair(&self, virtual_key: u16) -> Result<(), AXError> {
        unsafe extern "C" {
            fn AXUIElementPostKeyboardEvent(
                app: AXUIElementRef,
                key: u16,
                virtual_key: u16,
                down: u8,
            ) -> AXError;
        }
        let down = unsafe { AXUIElementPostKeyboardEvent(self.raw(), 0, virtual_key, 1) };
        let up = unsafe { AXUIElementPostKeyboardEvent(self.raw(), 0, virtual_key, 0) };
        if down != 0 {
            Err(down)
        } else if up != 0 {
            Err(up)
        } else {
            Ok(())
        }
    }

    pub fn system_wide() -> Element {
        // SAFETY: returns a +1 reference.
        let raw = unsafe { AXUIElementCreateSystemWide() };
        // SAFETY: raw is a valid +1 CF object.
        Self::bounded(unsafe { CFType::wrap_under_create_rule(raw as CFTypeRef) })
    }

    fn raw(&self) -> AXUIElementRef {
        self.0.as_CFTypeRef() as AXUIElementRef
    }

    pub fn settable(&self, name: &str) -> bool {
        let mut result = 0;
        unsafe {
            AXUIElementIsAttributeSettable(
                self.raw(),
                CFString::new(name).as_concrete_TypeRef(),
                &mut result,
            ) == 0
                && result != 0
        }
    }
    pub fn has_action(&self, name: &str) -> bool {
        let mut raw = std::ptr::null();
        if unsafe { AXUIElementCopyActionNames(self.raw(), &mut raw) } != 0 || raw.is_null() {
            return false;
        }
        let names: CFArray<CFString> = unsafe { CFArray::wrap_under_create_rule(raw) };
        names.iter().any(|n| n.to_string() == name)
    }
    pub fn action_names(&self) -> Vec<String> {
        let mut raw = std::ptr::null();
        if unsafe { AXUIElementCopyActionNames(self.raw(), &mut raw) } != 0 || raw.is_null() {
            return Vec::new();
        }
        let names: CFArray<CFString> = unsafe { CFArray::wrap_under_create_rule(raw) };
        names.iter().map(|n| n.to_string()).collect()
    }
    pub fn perform(&self, name: &str) -> Result<(), AXError> {
        let result = unsafe {
            AXUIElementPerformAction(self.raw(), CFString::new(name).as_concrete_TypeRef())
        };
        if result == 0 { Ok(()) } else { Err(result) }
    }
    pub fn set_timeout(&self, seconds: f32) {
        // SAFETY: valid element.
        unsafe { AXUIElementSetMessagingTimeout(self.raw(), seconds) };
    }

    pub fn pid(&self) -> Option<u32> {
        let mut pid = 0i32;
        // SAFETY: valid element and out-pointer.
        (unsafe { AXUIElementGetPid(self.raw(), &mut pid) } == 0 && pid > 0).then_some(pid as u32)
    }

    pub fn attribute(&self, name: &str) -> Option<CFType> {
        self.attribute_result(name).ok()
    }

    fn attribute_result(&self, name: &str) -> Result<CFType, AXError> {
        let key = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: valid element; value receives a +1 reference on success.
        let status = unsafe {
            AXUIElementCopyAttributeValue(self.raw(), key.as_concrete_TypeRef(), &mut value)
        };
        if status != 0 {
            if !value.is_null() {
                // SAFETY: release any +1 value returned with the failed read.
                drop(unsafe { CFType::wrap_under_create_rule(value) });
            }
            return Err(status);
        }
        if value.is_null() {
            return Err(-25200); // malformed success is not an absent attribute
        }
        // SAFETY: value is a valid +1 CF object.
        Ok(unsafe { CFType::wrap_under_create_rule(value) })
    }

    /// The AX status of reading `name`: 0, or why it failed
    /// (kAXErrorInvalidUIElement, -25202, once the element is gone).
    pub fn status(&self, name: &str) -> AXError {
        let key = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: valid element; value receives a +1 reference on success.
        let status = unsafe {
            AXUIElementCopyAttributeValue(self.raw(), key.as_concrete_TypeRef(), &mut value)
        };
        if !value.is_null() {
            // SAFETY: a +1 CF object this call owns.
            drop(unsafe { CFType::wrap_under_create_rule(value) });
        }
        status
    }

    pub fn string(&self, name: &str) -> Option<String> {
        let value = self.attribute(name)?;
        if let Some(s) = value.downcast::<CFString>() {
            return Some(s.to_string());
        }
        value
            .downcast::<CFURL>()
            .map(|u| u.get_string().to_string())
    }

    /// Several attributes in one round trip to the app. A missing attribute
    /// is `None` (the framework reports it as an error value in place).
    pub fn attributes(&self, names: &[&str]) -> Vec<Option<CFType>> {
        let Ok(values) = self.attribute_values(names) else {
            return vec![None; names.len()];
        };
        (0..names.len())
            // Preserve the existing best-effort projection for snapshot readers.
            .map(|i| values.get(i as isize).map(|v| v.clone()))
            .collect()
    }

    /// Like `attributes`, but preserve a failed IPC so a control scan cannot
    /// treat an unreadable subtree as an empty, successfully visited node.
    pub fn try_attributes(&self, names: &[&str]) -> Result<Vec<Option<CFType>>, AXError> {
        let values = self.attribute_values(names)?;
        if values.len() as usize != names.len() {
            return Err(-25200);
        }
        values
            .iter()
            .map(|value| checked_attribute(value.clone()))
            .collect()
    }

    fn attribute_values(&self, names: &[&str]) -> Result<CFArray<CFType>, AXError> {
        let keys: Vec<CFString> = names.iter().map(|n| CFString::new(n)).collect();
        let keys = CFArray::from_CFTypes(&keys);
        let mut raw: CFArrayRef = std::ptr::null();
        // SAFETY: valid element and array; `raw` receives a +1 array on success.
        let status = unsafe {
            AXUIElementCopyMultipleAttributeValues(
                self.raw(),
                keys.as_concrete_TypeRef(),
                0,
                &mut raw,
            )
        };
        if status != 0 {
            if !raw.is_null() {
                // SAFETY: release any +1 array returned with the failed call.
                drop(unsafe { CFArray::<CFType>::wrap_under_create_rule(raw) });
            }
            return Err(status);
        }
        if raw.is_null() {
            return Err(-25200);
        }
        // SAFETY: a +1 CFArray of CFTypes.
        Ok(unsafe { CFArray::wrap_under_create_rule(raw) })
    }

    /// An `AXChildren` value as elements.
    pub fn elements_of(value: &CFType) -> Vec<Element> {
        if value.type_of() != CFArray::<CFType>::type_id() {
            return Vec::new();
        }
        // SAFETY: the type id matched; get rule because `value` keeps its reference.
        let array: CFArray<CFType> =
            unsafe { CFArray::wrap_under_get_rule(value.as_CFTypeRef() as CFArrayRef) };
        // SAFETY: AXUIElementGetTypeID has no preconditions.
        let ax_type = unsafe { AXUIElementGetTypeID() };
        array
            .iter()
            .take(MAX_CHILDREN)
            .filter(|item| item.type_of() == ax_type)
            .map(|item| Self::bounded(item.clone()))
            .collect()
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        self.attribute(name)?
            .downcast::<CFBoolean>()
            .map(|b| b.into())
    }

    pub fn element(&self, name: &str) -> Option<Element> {
        self.element_result(name).ok()
    }

    pub fn element_result(&self, name: &str) -> Result<Element, AXError> {
        let value = self.attribute_result(name)?;
        // SAFETY: AXUIElementGetTypeID has no preconditions.
        if value.type_of() != unsafe { AXUIElementGetTypeID() } {
            return Err(-25200); // malformed type is not absence
        }
        Ok(Self::bounded(value))
    }

    pub fn children(&self) -> Vec<Element> {
        let Some(value) = self.attribute("AXChildren") else {
            return Vec::new();
        };
        if value.type_of() != CFArray::<CFType>::type_id() {
            return Vec::new();
        }
        // SAFETY: the type id matched; get rule because `value` keeps its reference.
        let array: CFArray<CFType> =
            unsafe { CFArray::wrap_under_get_rule(value.as_CFTypeRef() as CFArrayRef) };
        // SAFETY: AXUIElementGetTypeID has no preconditions.
        let ax_type = unsafe { AXUIElementGetTypeID() };
        array
            .iter()
            .take(MAX_CHILDREN)
            .filter(|item| item.type_of() == ax_type)
            .map(|item| Self::bounded(item.clone()))
            .collect()
    }

    pub fn role(&self) -> String {
        self.string("AXRole").unwrap_or_default()
    }

    pub fn set_string(&self, name: &str, text: &str) -> Result<(), AXError> {
        let key = CFString::new(name);
        let value = CFString::new(text);
        // SAFETY: valid element and CF objects.
        let status = unsafe {
            AXUIElementSetAttributeValue(
                self.raw(),
                key.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            )
        };
        if status == 0 { Ok(()) } else { Err(status) }
    }

    /// An integer attribute (`AXNumberOfCharacters`).
    pub fn number(&self, name: &str) -> Option<i64> {
        self.attribute(name)?
            .downcast::<core_foundation::number::CFNumber>()?
            .to_i64()
    }

    /// `AXStringForRange` for a range in UTF-16 units. `None` when the range
    /// lies outside the text.
    pub fn string_for_range(&self, location: usize, length: usize) -> Option<String> {
        let range = cf_range(location, length).ok()?;
        let key = CFString::new("AXStringForRange");
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: valid element, attribute and parameter; value receives a +1
        // reference on success.
        let status = unsafe {
            AXUIElementCopyParameterizedAttributeValue(
                self.raw(),
                key.as_concrete_TypeRef(),
                range.as_CFTypeRef(),
                &mut value,
            )
        };
        if status != 0 || value.is_null() {
            return None;
        }
        // SAFETY: value is a valid +1 CF object.
        let value = unsafe { CFType::wrap_under_create_rule(value) };
        value.downcast::<CFString>().map(|s| s.to_string())
    }

    /// Sets a text range attribute (`AXSelectedTextRange`), in UTF-16 units.
    pub fn set_range(&self, name: &str, location: usize, length: usize) -> Result<(), AXError> {
        let value = cf_range(location, length)?;
        let key = CFString::new(name);
        // SAFETY: valid element and CF objects.
        let status = unsafe {
            AXUIElementSetAttributeValue(
                self.raw(),
                key.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            )
        };
        if status == 0 { Ok(()) } else { Err(status) }
    }

    pub fn set_bool(&self, name: &str, on: bool) -> Result<(), AXError> {
        let key = CFString::new(name);
        let value = CFBoolean::from(on);
        // SAFETY: valid element and CF objects.
        let status = unsafe {
            AXUIElementSetAttributeValue(
                self.raw(),
                key.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            )
        };
        if status == 0 { Ok(()) } else { Err(status) }
    }

    pub fn raise(&self) -> Result<(), AXError> {
        let action = CFString::new("AXRaise");
        // SAFETY: valid retained element and action name.
        let status = unsafe { AXUIElementPerformAction(self.raw(), action.as_concrete_TypeRef()) };
        if status == 0 { Ok(()) } else { Err(status) }
    }

    pub fn press(&self) -> Result<(), AXError> {
        let action = CFString::new("AXPress");
        // SAFETY: valid element.
        let status = unsafe { AXUIElementPerformAction(self.raw(), action.as_concrete_TypeRef()) };
        if status == 0 { Ok(()) } else { Err(status) }
    }

    /// Read-only renderer query. Chromium serves ordinary AX attribute reads
    /// from its browser cache, but a hit test asks the renderer to update layout
    /// and serialize dirty AX objects, even under the opaque Locked Use cover.
    /// Discard the returned element: it is never a control/action authority.
    pub fn query_renderer(&self, web: &Element) -> bool {
        let Some(window) = web.element("AXWindow") else {
            return false;
        };
        let Some(point) = web
            .bounds()
            .zip(window.bounds())
            .and_then(|(web, window)| intersection_center(web, window))
        else {
            return false;
        };
        let mut raw = std::ptr::null();
        // SAFETY: valid application object, finite point inside its verified
        // web view/window, and an out pointer. This does not post mouse input.
        let status =
            unsafe { AXUIElementCopyElementAtPosition(self.raw(), point[0], point[1], &mut raw) };
        if !raw.is_null() {
            // SAFETY: Copy returns a +1 reference, released at end of scope.
            let _result = unsafe { CFType::wrap_under_create_rule(raw as CFTypeRef) };
        }
        status == 0
    }

    fn bounds(&self) -> Option<[f64; 4]> {
        let point = ax_pair(&self.attribute("AXPosition")?, 1)?;
        let size = ax_pair(&self.attribute("AXSize")?, 2)?;
        Some([point[0], point[1], size[0], size[1]])
    }

    /// Breadth-first over the subtree, deduplicated, bounded by nodes and time.
    pub fn walk(&self, mut visit: impl FnMut(&Element) -> bool) {
        self.walk_report(&mut visit);
    }

    /// Same traversal as `walk`, with an explicit completion reason for
    /// diagnostics. Reporting does not require a second AX traversal.
    pub fn walk_report(
        &self,
        mut visit: impl FnMut(&Element) -> bool,
    ) -> (usize, &'static str, u128) {
        self.walk_pruned(|node| {
            if visit(node) {
                Visit::Descend
            } else {
                Visit::Stop
            }
        })
    }

    /// Same traversal as `walk_report`, leaving out the subtrees the visitor
    /// skips. Skipped nodes still count towards the node ceiling.
    pub fn walk_pruned(
        &self,
        mut visit: impl FnMut(&Element) -> Visit,
    ) -> (usize, &'static str, u128) {
        let started = Instant::now();
        let mut queue = std::collections::VecDeque::from([self.clone()]);
        let mut seen: Vec<Element> = Vec::new();
        while let Some(node) = queue.pop_front() {
            if seen.len() >= MAX_NODES {
                return (seen.len(), "node_limit", started.elapsed().as_millis());
            }
            if started.elapsed() > WALK_BUDGET {
                return (seen.len(), "time_limit", started.elapsed().as_millis());
            }
            if seen.contains(&node) {
                continue;
            }
            seen.push(node.clone());
            match visit(&node) {
                Visit::Descend => queue.extend(node.children()),
                Visit::Skip => {}
                Visit::Stop => {
                    return (seen.len(), "visitor_stop", started.elapsed().as_millis());
                }
            }
        }
        (seen.len(), "complete", started.elapsed().as_millis())
    }

    /// A false visitor result prunes only that branch. Unlike `walk_report`,
    /// every sibling is still visited, and child-read failures are incomplete.
    pub fn walk_pruned_report(
        &self,
        visit: impl FnMut(&Element) -> bool,
    ) -> (usize, &'static str, u128) {
        muster_agent::native_tree::walk_pruned(
            self.clone(),
            MAX_NODES,
            WALK_BUDGET,
            |node| {
                let value = match node.attribute_result("AXChildren") {
                    Ok(value) => value,
                    Err(-25205 | -25212) => return Some(Vec::new()),
                    Err(_) => return None,
                };
                if value.type_of() != CFArray::<CFType>::type_id() {
                    return None;
                }
                // SAFETY: type checked; `value` keeps the array alive.
                let array: CFArray<CFType> =
                    unsafe { CFArray::wrap_under_get_rule(value.as_CFTypeRef() as CFArrayRef) };
                if array.len() as usize > MAX_CHILDREN {
                    return None;
                }
                // SAFETY: this type-id function has no preconditions.
                let ax_type = unsafe { AXUIElementGetTypeID() };
                if array.iter().any(|item| item.type_of() != ax_type) {
                    return None;
                }
                Some(
                    array
                        .iter()
                        .map(|item| Self::bounded(item.clone()))
                        .collect(),
                )
            },
            visit,
        )
    }

    /// A text listing of the subtree for diagnosis: depth, role, subrole,
    /// title, description, a short value, and the URL of web areas. Bounded
    /// by `limit` nodes and `budget`.
    pub fn dump(&self, limit: usize, budget: Duration) -> String {
        let started = Instant::now();
        let mut out = String::new();
        let mut stack: Vec<(Element, usize)> = vec![(self.clone(), 0)];
        let mut seen: Vec<Element> = Vec::new();
        while let Some((node, depth)) = stack.pop() {
            if seen.len() >= limit || started.elapsed() > budget {
                out.push_str("… (truncated)\n");
                break;
            }
            if seen.contains(&node) {
                continue;
            }
            seen.push(node.clone());
            let short = |s: Option<String>| -> String {
                let s = s.unwrap_or_default().replace('\n', "⏎");
                if s.chars().count() > 80 {
                    format!("{}…", s.chars().take(80).collect::<String>())
                } else {
                    s
                }
            };
            let role = node.role();
            let subrole = node.string("AXSubrole").unwrap_or_default();
            let title = short(node.string("AXTitle"));
            let desc = short(node.string("AXDescription"));
            let value = short(node.string("AXValue"));
            let url = short(node.string("AXURL"));
            let label = short(
                node.string("AXLabel")
                    .or_else(|| node.string("AXPlaceholderValue")),
            );
            let ident = short(
                node.string("AXIdentifier")
                    .or_else(|| node.string("AXDOMIdentifier")),
            );
            out.push_str(&format!(
                "{}{role}{}{}{}{}{}{}{}\n",
                "  ".repeat(depth),
                if subrole.is_empty() {
                    String::new()
                } else {
                    format!("/{subrole}")
                },
                if title.is_empty() {
                    String::new()
                } else {
                    format!(" title={title:?}")
                },
                if desc.is_empty() {
                    String::new()
                } else {
                    format!(" desc={desc:?}")
                },
                if label.is_empty() {
                    String::new()
                } else {
                    format!(" label={label:?}")
                },
                if value.is_empty() {
                    String::new()
                } else {
                    format!(" value={value:?}")
                },
                if url.is_empty() {
                    String::new()
                } else {
                    format!(" url={url:?}")
                },
                if ident.is_empty() {
                    String::new()
                } else {
                    format!(" id={ident:?}")
                },
            ));
            let children = node.children();
            for child in children.into_iter().rev() {
                stack.push((child, depth + 1));
            }
        }
        out
    }

    /// Whether `descendant` is somewhere under this element (Electron
    /// replaces nodes, so a retained element is re-checked before use).
    pub fn contains(&self, descendant: &Element) -> bool {
        let started = Instant::now();
        if muster_agent::native_send::reciprocal_ancestor(
            self,
            descendant,
            |node| node.element("AXParent"),
            |parent, child| parent.children().contains(child),
            || started.elapsed() < Duration::from_millis(100),
        ) {
            return true;
        }
        // Some native controls omit AXParent. Preserve the existing bounded
        // fresh scan as fallback; a stale parent alone never establishes membership.
        let mut found = false;
        self.walk(|node| {
            if node == descendant {
                found = true;
                return false;
            }
            true
        });
        found
    }

    /// Chromium exposes the full tree only once asked to (the POC's
    /// `AXManualAccessibility`); harmless where unsupported.
    pub fn enable_manual_accessibility(&self) {
        let _ = self.set_bool("AXManualAccessibility", true);
        let _ = self.set_bool("AXEnhancedUserInterface", true);
    }
}

/// A batch can succeed while individual attributes report errors. Missing
/// optional attributes are normal; a stale element/IPC failure is not an
/// empty subtree and must invalidate the scan.
fn checked_attribute(value: CFType) -> Result<Option<CFType>, AXError> {
    // SAFETY: valid CF object; GetValue checks the requested AX error type.
    if value.type_of() == unsafe { AXValueGetTypeID() } {
        let mut code: AXError = 0;
        if unsafe { AXValueGetValue(value.as_CFTypeRef(), 5, (&mut code as *mut AXError).cast()) } {
            return match code {
                -25205 | -25212 => Ok(None), // unsupported attribute / no value
                _ => Err(code),
            };
        }
    }
    Ok(Some(value))
}

/// A CFRange wrapped as an AXValue.
fn cf_range(location: usize, length: usize) -> Result<CFType, AXError> {
    #[repr(C)]
    struct CFRange {
        location: isize,
        length: isize,
    }
    const AX_VALUE_CF_RANGE: u32 = 4;
    let range = CFRange {
        location: location as isize,
        length: length as isize,
    };
    // SAFETY: AXValueCreate copies the CFRange it is given; the +1 result is
    // owned (and released) by the returned CFType.
    let raw = unsafe { AXValueCreate(AX_VALUE_CF_RANGE, (&range as *const CFRange).cast()) };
    if raw.is_null() {
        return Err(-25201);
    }
    // SAFETY: a valid +1 CF object from AXValueCreate.
    Ok(unsafe { CFType::wrap_under_create_rule(raw) })
}

/// A string, URL or number attribute value as text.
pub fn value_string(value: &CFType) -> Option<String> {
    if let Some(s) = value.downcast::<CFString>() {
        return Some(s.to_string());
    }
    if let Some(u) = value.downcast::<CFURL>() {
        return Some(u.get_string().to_string());
    }
    value
        .downcast::<core_foundation::number::CFNumber>()
        .and_then(|n| n.to_i64())
        .map(|n| n.to_string())
}

/// A boolean (or 0/1 number) attribute value.
pub fn value_bool(value: &CFType) -> Option<bool> {
    if let Some(b) = value.downcast::<CFBoolean>() {
        return Some(b.into());
    }
    value
        .downcast::<core_foundation::number::CFNumber>()
        .and_then(|n| n.to_i64())
        .map(|n| n != 0)
}

// CGPoint / CGSize both contain two CGFloat doubles on supported 64-bit Macs.
fn ax_pair(value: &CFType, kind: u32) -> Option<[f64; 2]> {
    if value.type_of() != unsafe { AXValueGetTypeID() } {
        return None;
    }
    let mut pair = [0.0; 2];
    // SAFETY: the AXValue type is checked; the API verifies its requested kind
    // and writes a CGPoint/CGSize into the equally sized double pair.
    unsafe { AXValueGetValue(value.as_CFTypeRef(), kind, pair.as_mut_ptr().cast()) }.then_some(pair)
}

fn intersection_center(a: [f64; 4], b: [f64; 4]) -> Option<[f32; 2]> {
    if !a.iter().chain(&b).all(|v| v.is_finite())
        || a[2] <= 0.0
        || a[3] <= 0.0
        || b[2] <= 0.0
        || b[3] <= 0.0
    {
        return None;
    }
    let left = a[0].max(b[0]);
    let top = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let bottom = (a[1] + a[3]).min(b[1] + b[3]);
    if left >= right || top >= bottom {
        return None;
    }
    let point = [
        (left + (right - left) / 2.0) as f32,
        (top + (bottom - top) / 2.0) as f32,
    ];
    point.iter().all(|v| v.is_finite()).then_some(point)
}

#[cfg(test)]
mod renderer_query_tests {
    use super::*;
    #[test]
    fn batch_attribute_errors_do_not_hide_an_unreadable_subtree() {
        let error = |code: AXError| {
            // SAFETY: AXValueCreate copies the error code; we own the +1 CF object.
            let raw = unsafe { AXValueCreate(5, (&code as *const AXError).cast()) };
            assert!(!raw.is_null());
            unsafe { CFType::wrap_under_create_rule(raw) }
        };
        for code in [-25205, -25212] {
            assert!(checked_attribute(error(code)).unwrap().is_none());
        }
        for code in [-25202, -25204, -25211] {
            assert_eq!(checked_attribute(error(code)).unwrap_err(), code);
        }
        let name = CFString::new("Send").as_CFType();
        assert_eq!(checked_attribute(name.clone()).unwrap(), Some(name));
    }

    #[test]
    fn clips_long_web_documents_to_their_window_without_rejecting_negative_screen_origins() {
        assert_eq!(
            intersection_center(
                [-900.0, -400.0, 1000.0, 9000.0],
                [-920.0, -450.0, 1040.0, 650.0]
            ),
            Some([-400.0, -100.0])
        );
        assert_eq!(
            intersection_center([0.0, 0.0, 100.0, 100.0], [200.0, 0.0, 100.0, 100.0]),
            None
        );
    }
    #[test]
    fn rejects_invalid_geometry_and_wrong_ax_types() {
        for invalid in [
            [0.0, 0.0, 0.0, 10.0],
            [0.0, 0.0, -1.0, 10.0],
            [f64::NAN, 0.0, 10.0, 10.0],
            [0.0, 0.0, f64::INFINITY, 10.0],
        ] {
            assert!(intersection_center(invalid, [0.0, 0.0, 10.0, 10.0]).is_none());
        }
        assert!(ax_pair(&CFString::new("not a point").as_CFType(), 1).is_none());
    }
}
