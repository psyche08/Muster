//! The platform's snapshot of an app's element tree: nodes in document
//! order, and the walks the interpreters below share.

/// One element of a snapshot. Strings are empty when the app does not
/// expose the attribute; `children` and `parent` index `AxTree::nodes`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AxNode {
    pub role: String,
    pub subrole: String,
    pub title: String,
    pub description: String,
    pub value: String,
    pub url: String,
    pub identifier: String,
    /// AXPlaceholderValue: what an empty prompt field shows.
    pub placeholder: String,
    pub enabled: bool,
    /// AXSelected, a radio button's AXValue of 1, or aria-current.
    pub selected: bool,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
}

impl AxNode {
    /// Title, else description: what a control is called.
    pub fn label(&self) -> &str {
        if self.title.is_empty() {
            &self.description
        } else {
            &self.title
        }
    }
}

/// A bounded snapshot of an application's element tree in document
/// order; node 0 is the application. `complete` is false when the walk
/// stopped at its node or time budget.
#[derive(Clone, Debug, Default)]
pub struct AxTree {
    pub nodes: Vec<AxNode>,
    pub complete: bool,
}

impl AxTree {
    /// Preorder (document order) below `root`, including it.
    pub fn preorder(&self, root: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(i) = stack.pop() {
            let Some(node) = self.nodes.get(i) else {
                continue;
            };
            out.push(i);
            stack.extend(node.children.iter().rev());
        }
        out
    }

    pub(super) fn ancestors(&self, mut i: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::from_fn(move || {
            let parent = self.nodes.get(i)?.parent?;
            i = parent;
            Some(parent)
        })
    }

    pub(super) fn inside(&self, i: usize, root: usize) -> bool {
        i == root || self.ancestors(i).any(|a| a == root)
    }

    /// The node's own text: a static text's value, else its label, else the
    /// concatenated static text below it.
    pub fn text_of(&self, i: usize) -> String {
        let node = &self.nodes[i];
        if node.role == "AXStaticText" {
            return if node.value.is_empty() {
                node.title.clone()
            } else {
                node.value.clone()
            };
        }
        if !node.label().is_empty() {
            return node.label().to_string();
        }
        self.preorder(i)
            .into_iter()
            .filter(|&j| self.nodes[j].role == "AXStaticText")
            .map(|j| self.nodes[j].value.clone())
            .collect::<String>()
    }
}
