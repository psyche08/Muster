#if os(macOS)
import SwiftUI
import AppKit
import MusterCore

struct RecycledConversation: NSViewControllerRepresentable {
    let chat: Muster_App_ChatView
    let header: AnyView
    let interactions: AnyView
    let showsInteractions: Bool
    let blockContent: (Muster_App_ChatBlock) -> AnyView
    let jumpToLatest: Int
    let holdPosition: Int
    @Binding var atLatest: Bool
    var loadEarlier: () -> Void = {}
    @Environment(\.self) private var environment
    @Environment(\.conversationStyle) private var style
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dynamicTypeSize) private var typeSize

    func makeNSViewController(context: Context) -> MacConversationController { MacConversationController() }
    func updateNSViewController(_ controller: MacConversationController, context: Context) {
        var environment = self.environment
        environment.transcriptDisclosures = controller.disclosures
        let binding = $atLatest
        controller.onLatest = { if binding.wrappedValue != $0 { binding.wrappedValue = $0 } }
        controller.onEarlier = loadEarlier
        controller.update(chat: chat, showsInteractions: showsInteractions,
            spacing: style == .claude ? 20 : 14, columnWidth: style.columnWidth,
            appearance: "\(style.rawValue):\(scheme):\(typeSize)", jump: jumpToLatest, hold: holdPosition,
            header: AnyView(header.environment(\.self, environment)),
            interactions: AnyView(interactions.environment(\.self, environment)),
            content: { AnyView(blockContent($0).environment(\.self, environment)) })
    }
}

final class MacConversationController: NSViewController {
    enum Row: Hashable { case header, block(String), interactions }
    struct Anchor { let row: Row; let offset: CGFloat }
    let disclosures = TranscriptDisclosures()
    let interactionHost: NSHostingView<AnyView> = TranscriptHostingView(rootView: AnyView(EmptyView()))
    private(set) var collection: MacConversationCollection!
    private(set) var scroll: NSScrollView!
    private var source: NSCollectionViewDiffableDataSource<Int, Row>!
    private var blocks: [String: Muster_App_ChatBlock] = [:]
    private var header = AnyView(EmptyView())
    private var content: ((Muster_App_ChatBlock) -> AnyView)?
    private var previousChat: Muster_App_ChatView?
    private var appearance = ""
    private var columnWidth: CGFloat = 760
    private var spacing: CGFloat = 14
    private var lastJump = 0
    private var lastHold = 0
    private var following = true
    private var anchor: Anchor?
    private var scheduled = false
    private var settling = false
    private var settlePass = 0
    private var updating = false
    private var historyTrigger = HistoryScrollTrigger()
    private var scrollObservers: [NSObjectProtocol] = []
    var onLatest: ((Bool) -> Void)?
    var onEarlier: (() -> Void)?

    override func loadView() {
        let layout = NSCollectionViewCompositionalLayout { [weak self] _, environment in
            let size = NSCollectionLayoutSize(widthDimension: .fractionalWidth(1), heightDimension: .estimated(100))
            let section = NSCollectionLayoutSection(group: .vertical(layoutSize: size, subitems: [NSCollectionLayoutItem(layoutSize: size)]))
            let inset = max(ChatStyle.inset, (environment.container.effectiveContentSize.width - (self?.columnWidth ?? 760)) / 2)
            section.contentInsets = .init(top: 12, leading: inset, bottom: 12, trailing: inset)
            return section
        }
        collection = MacConversationCollection()
        collection.collectionViewLayout = layout
        collection.backgroundColors = [.clear]
        collection.isSelectable = false
        collection.register(MacConversationItem.self, forItemWithIdentifier: .init("message"))
        scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        collection.autoresizingMask = [.width]
        scroll.documentView = collection
        view = scroll
        source = NSCollectionViewDiffableDataSource<Int, Row>(collectionView: collection) { [weak self] view, path, row in
            guard let self else { return nil }
            let item = view.makeItem(withIdentifier: .init("message"), for: path) as! MacConversationItem
            if row == .interactions { item.attach(self.interactionHost) }
            else {
                let rendered: AnyView
                switch row {
                case .header: rendered = self.header
                case .block(let id):
                    guard let block = self.blocks[id], let content = self.content else { return item }
                    rendered = content(block)
                case .interactions: return item
                }
                item.render(AnyView(rendered.id(row).environment(\.transcriptDisclosures, self.disclosures).padding(.bottom, self.spacing)))
            }
            return item
        }
        collection.afterLayout = { [weak self] in self?.scheduleSettle() }
        collection.userScroll = { [weak self] began, ended in self?.userScrolled(began: began, ended: ended) }
        // Native live-scroll notifications include trackpad, wheel and scroller
        // dragging, and keep the gesture alive through momentum scrolling.
        let center = NotificationCenter.default
        scrollObservers = [
            center.addObserver(forName: NSScrollView.willStartLiveScrollNotification, object: scroll, queue: .main) { [weak self] _ in self?.collection.beginScrolling() },
            center.addObserver(forName: NSScrollView.didLiveScrollNotification, object: scroll, queue: .main) { [weak self] _ in self?.userScrolled(began: false, ended: false) },
            center.addObserver(forName: NSScrollView.didEndLiveScrollNotification, object: scroll, queue: .main) { [weak self] _ in self?.collection.endScrolling() }
        ]
    }

    deinit { scrollObservers.forEach(NotificationCenter.default.removeObserver) }

    override func viewDidLayout() {
        super.viewDidLayout()
        let width = scroll.contentSize.width
        if abs(collection.frame.width - width) > 0.5 {
            collection.setFrameSize(NSSize(width: width, height: max(collection.frame.height, scroll.contentSize.height)))
            collection.collectionViewLayout?.invalidateLayout()
        }
        scheduleSettle()
    }

    func update(chat: Muster_App_ChatView, showsInteractions: Bool, spacing: CGFloat, columnWidth: CGFloat,
                appearance: String, jump: Int, hold: Int = 0, header: AnyView, interactions: AnyView,
                content: @escaping (Muster_App_ChatBlock) -> AnyView) {
        _ = view
        if hold != lastHold { following = false; lastHold = hold; captureAnchor() }
        if !following && anchor == nil { captureAnchor() }
        let restyle = self.appearance != appearance || self.columnWidth != columnWidth
        self.appearance = appearance; self.columnWidth = columnWidth; self.spacing = spacing
        self.header = header; self.content = content
        let old = blocks
        blocks = Dictionary(chat.timeline.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        let rows: [Row] = [.header] + chat.timeline.map { .block($0.id) } + (showsInteractions && !chat.interactions.isEmpty ? [.interactions] : [])
        let previousRows = source.snapshot().itemIdentifiers
        let changed = rows.filter { row in
            switch row {
            case .header: return restyle || previousChat?.loading != chat.loading || previousChat?.loadError != chat.loadError || previousChat?.hasEarlier_p != chat.hasEarlier_p || previousChat?.messageCount != chat.messageCount || previousChat?.session.transcriptNotice != chat.session.transcriptNotice
            case .block(let id): return restyle || old[id] != blocks[id]
            case .interactions: return false
            }
        }
        if previousChat?.interactions != chat.interactions || previousChat?.respondingInteraction != chat.respondingInteraction || previousChat?.responseError != chat.responseError || previousChat?.responseUnknown != chat.responseUnknown || restyle {
            interactionHost.rootView = interactions
        }
        previousChat = chat
        if lastJump != jump { following = true; anchor = nil; lastJump = jump }
        updating = true
        if rows != previousRows || !changed.isEmpty {
            var snapshot = NSDiffableDataSourceSnapshot<Int, Row>()
            snapshot.appendSections([0]); snapshot.appendItems(rows)
            let existing = Set(previousRows)
            snapshot.reloadItems(changed.filter { existing.contains($0) })
            source.apply(snapshot, animatingDifferences: false)
        }
        if restyle { collection.collectionViewLayout?.invalidateLayout() }
        updating = false
        scheduleSettle()
    }

    private func captureAnchor() {
        let top = scroll.contentView.bounds.minY
        let visible = collection.indexPathsForVisibleItems().sorted().filter {
            (collection.layoutAttributesForItem(at: $0)?.frame.maxY ?? 0) > top
        }
        let message = visible.first { if case .block = source.itemIdentifier(for: $0) { return true }; return false }
        guard let path = message ?? visible.first, let row = source.itemIdentifier(for: path),
              let frame = collection.layoutAttributesForItem(at: path)?.frame else { return }
        anchor = Anchor(row: row, offset: frame.minY - top)
    }

    private func scheduleSettle() {
        guard !scheduled, !settling else { return }
        scheduled = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.scheduled = false; self.settleViewport()
        }
    }
    private func settleViewport() {
        guard !updating, !collection.scrolling else { return }
        settling = true
        var changedGeometry = false
        defer {
            settling = false
            if changedGeometry && settlePass < 6 { settlePass += 1; scheduleSettle() }
            else { settlePass = 0 }
        }
        collection.layoutSubtreeIfNeeded()
        let height = max(scroll.contentSize.height, collection.collectionViewLayout!.collectionViewContentSize.height)
        if abs(collection.frame.height - height) > 0.5 {
            changedGeometry = true
            collection.setFrameSize(NSSize(width: scroll.contentSize.width, height: height))
            collection.layoutSubtreeIfNeeded()
        }
        let maximum = max(0, collection.collectionViewLayout!.collectionViewContentSize.height - scroll.contentView.bounds.height)
        var target: CGFloat?
        if following { target = maximum }
        else if let anchor, let path = source.indexPath(for: anchor.row), let frame = collection.layoutAttributesForItem(at: path)?.frame {
            target = min(maximum, max(0, frame.minY - anchor.offset))
        }
        if let target, abs(scroll.contentView.bounds.minY - target) > 0.5 {
            changedGeometry = true
            scroll.contentView.scroll(to: NSPoint(x: 0, y: target)); scroll.reflectScrolledClipView(scroll.contentView)
        }
        onLatest?(following)
    }
    private func userScrolled(began: Bool, ended: Bool) {
        guard !updating, !settling else { return }
        if began { historyTrigger.beginGesture(); following = false; anchor = nil }
        captureAnchor()
        if let chat = previousChat, historyTrigger.request(offset: scroll.contentView.bounds.minY,
            hasEarlier: chat.hasEarlier_p, loading: chat.loading, failed: !chat.loadError.isEmpty) {
            onEarlier?()
        }
        if ended {
            following = collection.collectionViewLayout!.collectionViewContentSize.height - scroll.contentView.bounds.height - scroll.contentView.bounds.minY <= 48
            scheduleSettle()
        }
        onLatest?(following)
    }
}

final class MacConversationCollection: NSCollectionView {
    var afterLayout: (() -> Void)?
    var userScroll: ((_ began: Bool, _ ended: Bool) -> Void)?
    private(set) var scrolling = false
    override var isFlipped: Bool { true }
    override func layout() { super.layout(); afterLayout?() }
    func beginScrolling() { scrolling = true; userScroll?(true, false) }
    func endScrolling() { scrolling = false; userScroll?(false, true) }

}

final class MacConversationItem: NSCollectionViewItem {
    private var host: TranscriptHostingView?
    private weak var retained: NSView?
    private var pins: [NSLayoutConstraint] = []
    override func loadView() { view = NSView() }
    func render(_ content: AnyView) {
        detach()
        let host = self.host ?? TranscriptHostingView(rootView: content)
        host.rootView = content; self.host = host
        pin(host)
    }
    func attach(_ content: NSView) { detach(); host = nil; retained = content; pin(content) }
    private func pin(_ content: NSView) {
        (content as? TranscriptHostingView)?.sizeChanged = { [weak self] in self?.invalidateHeight() }
        content.removeFromSuperview(); content.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(content)
        pins = [content.leadingAnchor.constraint(equalTo: view.leadingAnchor), content.trailingAnchor.constraint(equalTo: view.trailingAnchor), content.topAnchor.constraint(equalTo: view.topAnchor), content.bottomAnchor.constraint(equalTo: view.bottomAnchor)]
        NSLayoutConstraint.activate(pins)
    }
    private func detach() {
        NSLayoutConstraint.deactivate(pins); pins = []
        if retained?.superview === view { retained?.removeFromSuperview() }
        retained = nil; host?.removeFromSuperview()
    }
    private var heightCheckScheduled = false
    private func invalidateHeight() {
        guard !heightCheckScheduled else { return }
        heightCheckScheduled = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.heightCheckScheduled = false
            guard let collection = self.collectionView,
                  let path = collection.indexPath(for: self),
                  let attributes = collection.layoutAttributesForItem(at: path),
                  abs(self.view.fittingSize.height - attributes.size.height) > 0.5 else { return }
            collection.collectionViewLayout?.invalidateLayout()
        }
    }
    override func prepareForReuse() { super.prepareForReuse(); detach(); host?.rootView = AnyView(EmptyView()) }
    override func preferredLayoutAttributesFitting(_ attributes: NSCollectionViewLayoutAttributes) -> NSCollectionViewLayoutAttributes {
        let result = attributes.copy() as! NSCollectionViewLayoutAttributes
        view.setFrameSize(NSSize(width: attributes.size.width, height: attributes.size.height))
        view.layoutSubtreeIfNeeded()
        result.size.height = max(1, view.fittingSize.height)
        return result
    }
}
private final class TranscriptHostingView: NSHostingView<AnyView> {
    var sizeChanged: (() -> Void)?
    override func invalidateIntrinsicContentSize() {
        super.invalidateIntrinsicContentSize()
        sizeChanged?()
    }
}
#endif
