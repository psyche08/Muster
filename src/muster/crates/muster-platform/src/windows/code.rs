//! Adapt public Windows package activation and UI Automation to Code threads.
use super::{
    chat::{self, Element},
    link_clipboard, provider,
};
use muster_agent::{
    Error, control_guard,
    controller::{Controller, ProcessIdentity, ProcessState},
    desktop_chat::{AxNode, AxTree},
    model::{ControllerKind, PendingInteraction, SessionState},
    native_code_send::{self, Window},
    native_send, native_sidebar, receipt,
    unconfirmed::WriteGuard,
};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Foundation::Uri,
    System::{Launcher, LauncherOptions},
    Win32::UI::Accessibility::*,
    core::{BSTR, HSTRING},
};
fn unavailable() -> Error {
    Error::Unavailable("Codex desktop does not expose the required thread controls".into())
}
fn uia(_: windows::core::Error) -> Error {
    unavailable()
}
fn unique<T>(items: impl IntoIterator<Item = T>) -> Result<T, Error> {
    let mut iter = items.into_iter();
    let one = iter.next().ok_or_else(unavailable)?;
    if iter.next().is_some() {
        return Err(Error::Conflict(
            "Codex desktop controls are ambiguous".into(),
        ));
    }
    Ok(one)
}
struct Surface {
    identity: ProcessIdentity,
    input: u64,
    root: Element,
}
struct Tree {
    ax: AxTree,
    elements: Vec<Element>,
}
impl Tree {
    fn ancestor(&self, index: usize, pred: impl Fn(&AxNode) -> bool) -> bool {
        let mut parent = self.ax.nodes[index].parent;
        while let Some(i) = parent {
            if pred(&self.ax.nodes[i]) {
                return true;
            }
            parent = self.ax.nodes[i].parent;
        }
        false
    }
    fn control(&self, index: usize) -> Result<IUIAutomationElement, Error> {
        let node = self.elements[index].control().map_err(uia)?;
        if unsafe { node.CurrentIsOffscreen().map_err(uia)? }.as_bool() {
            return Err(unavailable());
        }
        Ok(node)
    }
    fn named(
        &self,
        roles: &[&str],
        names: &[&str],
        ancestor: impl Fn(&AxNode) -> bool,
    ) -> Result<Element, Error> {
        let index = unique(
            self.ax
                .nodes
                .iter()
                .enumerate()
                .filter(|(i, n)| {
                    n.enabled
                        && roles.contains(&n.role.as_str())
                        && names.contains(&n.title.as_str())
                        && self.ancestor(*i, &ancestor)
                })
                .map(|(i, _)| i),
        )?;
        self.control(index)?;
        Ok(self.elements[index].clone())
    }
    fn composer(&self) -> Result<(usize, usize, IUIAutomationValuePattern), Error> {
        let mut matches = Vec::new();
        for (i, _) in self
            .ax
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.enabled && n.role == "AXTextArea")
        {
            let node = self.elements[i].control().map_err(uia)?;
            if unsafe { node.CurrentIsOffscreen().map_err(uia)? }.as_bool() {
                continue;
            }
            let class = unsafe { node.CurrentClassName().map_err(uia)? }.to_string();
            if class.split_ascii_whitespace().any(|c| c == "ProseMirror") {
                let value = unsafe {
                    node.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                }
                .map_err(uia)?;
                if unsafe { value.CurrentIsReadOnly().map_err(uia)? }.as_bool() {
                    return Err(unavailable());
                }
                matches.push((i, value));
            }
        }
        let (i, value) = unique(matches)?;
        // Keep the editor in the provider's thread layout and out of rendered message bodies.
        let mut footer = None;
        let mut parent = self.ax.nodes[i].parent;
        while let Some(p) = parent {
            let n = &self.ax.nodes[p];
            if n.subrole == "AXDocumentArticle" || n.subrole == "AXApplicationDialog" {
                return Err(unavailable());
            }
            let node = self.elements[p].control().map_err(uia)?;
            let class = unsafe { node.CurrentClassName().map_err(uia)? }.to_string();
            if footer.is_none()
                && class
                    .split_ascii_whitespace()
                    .any(|c| c.starts_with("footer-"))
            {
                footer = Some(p);
            }
            if class
                .split_ascii_whitespace()
                .any(|c| c == "group/thread-scroll-layout")
            {
                return Ok((i, footer.ok_or_else(unavailable)?, value));
            }
            parent = n.parent;
        }
        Err(unavailable())
    }
}
impl Surface {
    fn tree(&self) -> Result<Tree, Error> {
        self.check()?;
        let (ax, elements) = chat::snapshot(&self.root, false);
        if !ax.complete
            || ax.nodes.is_empty()
            || ax.nodes.iter().any(|n| n.subrole == "AXApplicationDialog")
        {
            return Err(unavailable());
        }
        Ok(Tree { ax, elements })
    }
    fn wait_menu(&self, names: &[&str], parent: &[&str]) -> Result<Element, Error> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let tree = self.tree()?;
            if let Ok(el) = tree.named(&["AXMenuItem"], names, |n| {
                n.role == "AXMenu" && parent.contains(&n.title.as_str())
            }) {
                return Ok(el);
            }
            if Instant::now() >= deadline {
                return Err(unavailable());
            }
            thread::sleep(Duration::from_millis(80));
        }
    }
    fn expand(&self, el: &Element) -> Result<IUIAutomationExpandCollapsePattern, Error> {
        let pattern = unsafe {
            el.control()
                .map_err(uia)?
                .GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(
                    UIA_ExpandCollapsePatternId,
                )
        }
        .map_err(uia)?;
        if unsafe { pattern.CurrentExpandCollapseState() }.map_err(uia)?
            != ExpandCollapseState_Collapsed
        {
            return Err(unavailable());
        }
        self.check()?;
        unsafe { pattern.Expand() }.map_err(uia)?;
        Ok(pattern)
    }
    fn copied_link(&self, tree: &Tree) -> Result<String, Error> {
        let actions = ["Chat actions", "聊天操作", "对话操作"];
        let copy = ["Copy", "复制"];
        let header = tree.named(&["AXButton", "AXPopUpButton"], &actions, |n| {
            n.role == "AXToolbar"
        })?;
        let opened = self.expand(&header).map_err(|_| {
            Error::Unavailable("Close any open Codex menu and retry when the chat is idle".into())
        })?;
        let result = (|| {
            let item = self.wait_menu(&copy, &actions).map_err(|_| {
                Error::Unavailable("Codex did not expose Copy in Chat actions".into())
            })?;
            self.expand(&item)?;
            let link = self
                .wait_menu(&["Copy deeplink", "复制深层链接", "复制深度链接"], &copy)
                .map_err(|_| {
                    Error::Unavailable(
                        "Codex did not expose the conversation deeplink action".into(),
                    )
                })?;
            link_clipboard::copied(self.identity.pid, || {
                self.check()?;
                link.press().map_err(uia)
            })
        })();
        if result.is_err() && self.check().is_ok() {
            unsafe {
                let _ = opened.Collapse();
            }
        }
        result
    }
}
impl Window for Surface {
    type Editor = Element;
    type Submit = IUIAutomationInvokePattern;
    fn check(&self) -> Result<(), Error> {
        control_guard::check()?;
        chat::console_ready()?;
        chat::window(&self.root)?;
        if provider::process_start(self.identity.pid) != Some(self.identity.start)
            || chat::target_input_generation(self.identity.pid) != self.input
        {
            return Err(Error::Conflict(
                "Codex changed or local input interrupted the operation".into(),
            ));
        }
        Ok(())
    }
    fn navigate(&self, route: &str) -> Result<(), Error> {
        let uri = Uri::CreateUri(&HSTRING::from(route)).map_err(uia)?;
        let options = LauncherOptions::new().map_err(uia)?;
        options
            .SetTargetApplicationPackageFamilyName(&HSTRING::from(provider::CODE_PACKAGE))
            .map_err(uia)?;
        self.check()?;
        let launch = Launcher::LaunchUriWithOptionsAsync(&uri, &options).map_err(uia)?;
        let deadline = Instant::now() + Duration::from_secs(4);
        while launch.Status().map_err(uia)?.0 == 0 {
            if Instant::now() >= deadline || self.check().is_err() {
                let _ = launch.Cancel();
                return Err(unavailable());
            }
            thread::sleep(Duration::from_millis(30));
        }
        if !launch.GetResults().map_err(uia)? {
            return Err(unavailable());
        }
        Ok(())
    }
    fn thread_link(&self) -> Result<String, Error> {
        let tree = self.tree()?;
        let doc = unique(tree.ax.nodes.iter().enumerate().filter(|(i, n)| {
            n.role == "AXWebArea" && !tree.ancestor(*i, |n| n.role == "AXWebArea")
        }))?;
        tree.control(doc.0)?;
        if native_sidebar::codex_thread_id(&doc.1.url).is_some() {
            return Ok(doc.1.url.clone());
        }
        self.copied_link(&tree)
    }
    fn editor(&self) -> Result<(Element, String), Error> {
        let tree = self.tree()?;
        if tree.ax.nodes.iter().any(|n| {
            n.enabled
                && matches!(n.role.as_str(), "AXButton" | "AXPopUpButton")
                && matches!(
                    n.label(),
                    "Stop"
                        | "Stop response"
                        | "Stop generating"
                        | "Steer"
                        | "Queue"
                        | "停止"
                        | "排队"
                )
        }) {
            return Err(Error::Conflict("Codex is already processing a turn".into()));
        }
        let (i, _, value) = tree.composer()?;
        Ok((
            tree.elements[i].clone(),
            unsafe { value.CurrentValue() }.map_err(uia)?.to_string(),
        ))
    }
    fn write(&self, editor: &Element, text: &str) -> Result<(), Error> {
        let value = unsafe {
            editor
                .control()
                .map_err(uia)?
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        }
        .map_err(uia)?;
        if !unsafe { value.CurrentValue() }.map_err(uia)?.is_empty() {
            return Err(Error::Conflict("Codex has an existing draft".into()));
        }
        self.check()?;
        unsafe { value.SetValue(&BSTR::from(text)) }.map_err(uia)
    }
    fn submit_control(&self, editor: &Element, text: &str) -> Result<Self::Submit, Error> {
        let tree = self.tree()?;
        let (i, footer, value) = tree.composer()?;
        let client = chat::client().map_err(uia)?;
        let same = unsafe {
            client.CompareElements(
                &editor.control().map_err(uia)?,
                &tree.elements[i].control().map_err(uia)?,
            )
        }
        .map_err(uia)?;
        if !same.as_bool() || unsafe { value.CurrentValue() }.map_err(uia)?.to_string() != text {
            return Err(Error::Conflict(
                "The Codex composer changed before submission".into(),
            ));
        }
        let nodes: Vec<_> = tree
            .ax
            .nodes
            .iter()
            .map(|n| native_send::ControlNode {
                parent: n.parent,
                composer: n.role == "AXTextArea",
                submit: n.role == "AXButton"
                    && n.enabled
                    && matches!(n.label(), "Send" | "Send message" | "发送" | "发送消息"),
                queued_delete: n.role == "AXButton"
                    && matches!(n.label(), "Delete queued message" | "删除排队消息"),
            })
            .collect();
        let index = unique(native_send::composer_submits_for(&nodes, i))?;
        if !tree.ax.preorder(footer).contains(&index) {
            return Err(unavailable());
        }
        unsafe {
            tree.control(index)?
                .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
        }
        .map_err(uia)
    }
    fn press(&self, button: &Self::Submit) -> Result<(), Error> {
        self.check()?;
        unsafe { button.Invoke() }.map_err(uia)
    }
}

#[derive(Clone)]
struct Code {
    process: ProcessIdentity,
    session: String,
    source: receipt::Source,
    pending: Arc<WriteGuard>,
}
// Only one operation may drive the app, even when requests name different threads.
static WINDOW: Mutex<()> = Mutex::new(());
pub fn attach_code(session: String, source: receipt::Source) -> Result<Arc<dyn Controller>, Error> {
    let (pid, start) = provider::code_process().ok_or_else(|| {
        Error::Unavailable("Open the official Codex desktop app in one window".into())
    })?;
    Ok(Arc::new(Code {
        process: ProcessIdentity { pid, start },
        session,
        source,
        pending: Arc::new(WriteGuard::default()),
    }))
}
#[async_trait::async_trait]
impl Controller for Code {
    fn kind(&self) -> ControllerKind {
        ControllerKind::NativeApp
    }
    fn session_id(&self) -> String {
        self.session.clone()
    }
    fn state(&self) -> SessionState {
        SessionState::Unknown
    }
    fn process_identity(&self) -> ProcessIdentity {
        self.process
    }
    fn process_state(&self) -> ProcessState {
        self.process
            .observe(provider::observe_process_start(self.process.pid).map_err(|_| ()))
    }
    fn pending(&self) -> Vec<PendingInteraction> {
        Vec::new()
    }
    fn reports_delivery(&self) -> bool {
        true
    }
    async fn lookup_send(&self, operation: &str) -> Result<bool, Error> {
        let guard = self.pending.clone();
        let token = operation.to_owned();
        control_guard::spawn_blocking(move || matches!(guard.reconcile(),
            muster_agent::unconfirmed::Reconciled::Confirmed(origin,_) if origin.operation.as_deref()==Some(&token)))
            .await
            .map_err(|_| unavailable())
    }
    fn release_unconfirmed(&self, op: &str) -> Result<(), Error> {
        self.pending.release(op)
    }
    fn confirm_delivered(
        &self,
        op: &str,
        persist: &control_guard::Confirmation,
    ) -> Result<(), Error> {
        self.pending.user_confirm(op, persist)
    }
    async fn send(&self, text: &str) -> Result<(), Error> {
        let this = self.clone();
        let text = text.to_owned();
        control_guard::spawn_blocking(move || {
            let _lock = WINDOW
                .try_lock()
                .map_err(|_| Error::Conflict("Another Codex desktop send is in progress".into()))?;
            let pid = this.process.pid;
            let surface = Surface {
                identity: this.process,
                input: chat::target_input_generation(pid),
                root: chat::chat_window(pid)?,
            };
            native_code_send::send(&surface, &this.session, &text, &this.source, &this.pending)
        })
        .await
        .map_err(|_| unavailable())?
    }
    async fn respond(&self, _: &str, _: &str, _: &str) -> Result<(), Error> {
        Err(unavailable())
    }
    async fn interrupt(&self) -> Result<(), Error> {
        Err(unavailable())
    }
    async fn stop(&self) {}
}
