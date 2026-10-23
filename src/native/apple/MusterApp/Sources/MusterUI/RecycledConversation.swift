#if os(iOS)
import SwiftUI
import UIKit
import MusterCore

/// UIKit owns only row layout and transient presentation state. Rust supplies the complete
/// presentation window and every load/send/answer decision.
struct RecycledConversation: UIViewControllerRepresentable {
    let chat: Muster_App_ChatView
    let header: AnyView
    let interactions: AnyView
    let showsInteractions: Bool
    let blockContent: (Muster_App_ChatBlock) -> AnyView
    let jumpToLatest: Int
    let holdPosition: Int
    @Binding var atLatest: Bool
    @Environment(\.self) private var environment
    @Environment(\.conversationStyle) private var style
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dynamicTypeSize) private var typeSize

    func makeUIViewController(context: Context) -> ConversationCollectionController {
        ConversationCollectionController()
    }

    func updateUIViewController(_ controller: ConversationCollectionController, context: Context) {
        var environment = self.environment
        environment.transcriptDisclosures = controller.disclosures
        let binding = $atLatest
        controller.onLatest = { value in if binding.wrappedValue != value { binding.wrappedValue = value } }
        controller.update(chat: chat, showsInteractions: showsInteractions,
                          spacing: style == .claude ? 20 : 14, columnWidth: style.columnWidth,
                          appearance: "\(style.rawValue):\(scheme):\(typeSize)", jump: jumpToLatest, hold: holdPosition,
                          header: AnyView(header.environment(\.self, environment)),
                          interactions: AnyView(interactions.environment(\.self, environment)),
                          content: { AnyView(blockContent($0).environment(\.self, environment)) })
    }
}

final class ConversationCollectionController: UIViewController, UICollectionViewDelegate {
    let disclosures = TranscriptDisclosures()
    enum Row: Hashable { case header, block(String), interactions }
    struct Anchor { let row: Row; let offset: CGFloat }
    private(set) var collection: ConversationCollectionView!
    private var source: UICollectionViewDiffableDataSource<Int, Row>!
    private var blocks: [String: Muster_App_ChatBlock] = [:]
    private var content: ((Muster_App_ChatBlock) -> AnyView)?
    private var header = AnyView(EmptyView())
    // Unlike transcript cells, this one host keeps unsent question input alive
    // while its cell is offscreen. It contains only the current interaction area.
    let interactionHost = UIHostingController(rootView: AnyView(EmptyView()))
    private var previousChat: Muster_App_ChatView?
    private var appearance = ""
    private var lastJump = 0
    private var lastHold = 0
    private var anchor: Anchor?
    private var following = true
    private var updating = false
    private var settling = false
    private var scheduled = false
    private var columnWidth: CGFloat = 760
    var onLatest: ((Bool) -> Void)?

    override func loadView() {
        let layout = UICollectionViewCompositionalLayout { [weak self] _, environment in
            let item = NSCollectionLayoutItem(layoutSize: .init(widthDimension: .fractionalWidth(1), heightDimension: .estimated(100)))
            let group = NSCollectionLayoutGroup.vertical(layoutSize: .init(widthDimension: .fractionalWidth(1), heightDimension: .estimated(100)), subitems: [item])
            let section = NSCollectionLayoutSection(group: group)
            let inset = max(ChatStyle.inset, (environment.container.effectiveContentSize.width - (self?.columnWidth ?? 760)) / 2)
            section.contentInsets = .init(top: 12, leading: inset, bottom: 12, trailing: inset)
            return section
        }
        collection = ConversationCollectionView(frame: .zero, collectionViewLayout: layout)
        collection.backgroundColor = .clear
        collection.alwaysBounceVertical = true
        collection.keyboardDismissMode = .interactive
        collection.contentInsetAdjustmentBehavior = .never
        collection.selfSizingInvalidation = .enabledIncludingConstraints
        collection.accessibilityIdentifier = "conversation-collection"
        collection.delegate = self
        collection.register(UICollectionViewCell.self, forCellWithReuseIdentifier: "message")
        collection.register(InteractionHostingCell.self, forCellWithReuseIdentifier: "interaction")
        view = collection
        addChild(interactionHost)
        interactionHost.sizingOptions = [.intrinsicContentSize]
        interactionHost.view.backgroundColor = .clear
        interactionHost.didMove(toParent: self)
        source = UICollectionViewDiffableDataSource<Int, Row>(collectionView: collection) { [weak self] view, path, row in
            guard let self else { return nil }
            if row == .interactions {
                let cell = view.dequeueReusableCell(withReuseIdentifier: "interaction", for: path) as! InteractionHostingCell
                cell.attach(self.interactionHost.view)
                return cell
            }
            let cell = view.dequeueReusableCell(withReuseIdentifier: "message", for: path)
            let rendered: AnyView
            switch row {
            case .header: rendered = self.header
            case .block(let id):
                guard let block = self.blocks[id], let content = self.content else { return cell }
                rendered = content(block)
            case .interactions: return cell
            }
            let spacing = self.rowSpacing
            let disclosures = self.disclosures
            cell.contentConfiguration = UIHostingConfiguration {
                rendered.id(row).environment(\.transcriptDisclosures, disclosures).padding(.bottom, spacing)
            }.margins(.all, 0).minSize(height: 1)
            return cell
        }
        collection.afterLayout = { [weak self] in self?.scheduleSettle() }
    }

    private var rowSpacing: CGFloat = 14

    func update(chat: Muster_App_ChatView, showsInteractions: Bool, spacing: CGFloat, columnWidth: CGFloat,
                appearance: String, jump: Int, hold: Int = 0, header: AnyView, interactions: AnyView,
                content: @escaping (Muster_App_ChatBlock) -> AnyView) {
        loadViewIfNeeded()
        if lastHold != hold { following = false; lastHold = hold; captureAnchor() }
        // Captured before applying a new projection: index paths can move when
        // Rust reveals older history, but the anchored row ID remains stable.
        if !following && !collection.isDragging && !collection.isDecelerating { captureAnchor() }
        let restyle = self.appearance != appearance || self.columnWidth != columnWidth
        self.appearance = appearance; self.columnWidth = columnWidth; rowSpacing = spacing
        self.header = header; self.content = content
        let old = blocks
        blocks = Dictionary(chat.timeline.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        let ids = chat.timeline.map { Row.block($0.id) }
        let rows: [Row] = [.header] + ids + (showsInteractions && !chat.interactions.isEmpty ? [.interactions] : [])
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
            interactionHost.view.invalidateIntrinsicContentSize()
        }
        previousChat = chat
        if lastJump != jump { following = true; anchor = nil; lastJump = jump }
        updating = true
        if rows != previousRows {
            var snapshot = NSDiffableDataSourceSnapshot<Int, Row>()
            snapshot.appendSections([0]); snapshot.appendItems(rows)
            let existing = Set(previousRows)
            snapshot.reconfigureItems(changed.filter { existing.contains($0) })
            source.apply(snapshot, animatingDifferences: false)
        } else if !changed.isEmpty {
            var snapshot = source.snapshot(); snapshot.reconfigureItems(changed)
            source.apply(snapshot, animatingDifferences: false)
        }
        if restyle { collection.collectionViewLayout.invalidateLayout() }
        updating = false
        scheduleSettle()
    }

    private func scheduleSettle() {
        guard !scheduled, !settling else { return }
        scheduled = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.scheduled = false
            self.settleViewport()
        }
    }

    private func settleViewport() {
        guard !updating, !collection.isDragging, !collection.isDecelerating else { return }
        settling = true
        defer { settling = false }
        collection.layoutIfNeeded()
        let minY = -collection.adjustedContentInset.top
        let maxY = max(minY, collection.contentSize.height - collection.bounds.height + collection.adjustedContentInset.bottom)
        var target: CGFloat?
        if following { target = maxY }
        else if let anchor, let path = source.indexPath(for: anchor.row), let frame = collection.layoutAttributesForItem(at: path)?.frame {
            target = min(maxY, max(minY, frame.minY - anchor.offset))
        }
        if let target, abs(collection.contentOffset.y - target) > 0.5 {
            collection.setContentOffset(CGPoint(x: 0, y: target), animated: false)
            // Scrolling to estimated offscreen rows makes them self-size. The
            // next actual layout callback reconciles the same ID/offset again.
        }
        onLatest?(following)
    }

    private func captureAnchor() {
        let top = collection.contentOffset.y + collection.adjustedContentInset.top
        let visible = collection.indexPathsForVisibleItems.sorted().filter {
            (collection.layoutAttributesForItem(at: $0)?.frame.maxY ?? 0) > top
        }
        // The load-earlier header stays at index zero when history is inserted;
        // anchoring that header would move every message the reader can see.
        let message = visible.first { if case .block = source.itemIdentifier(for: $0) { return true }; return false }
        guard let path = message ?? visible.first,
              let row = source.itemIdentifier(for: path), let frame = collection.layoutAttributesForItem(at: path)?.frame else { return }
        anchor = Anchor(row: row, offset: frame.minY - collection.contentOffset.y)
    }

    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        following = false; anchor = nil; onLatest?(false)
    }
    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        guard !updating, !settling, scrollView.isDragging || scrollView.isDecelerating else { return }
        captureAnchor()
    }
    func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool) {
        if !decelerate { finishedScrolling() }
    }
    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) { finishedScrolling() }
    private func finishedScrolling() {
        following = collection.contentSize.height - collection.bounds.height + collection.adjustedContentInset.bottom - collection.contentOffset.y <= 48
        captureAnchor(); onLatest?(following)
        scheduleSettle()
    }
}

final class ConversationCollectionView: UICollectionView {
    var afterLayout: (() -> Void)?
    override func layoutSubviews() { super.layoutSubviews(); afterLayout?() }
}

private final class InteractionHostingCell: UICollectionViewCell {
    private weak var hosted: UIView?
    private var pins: [NSLayoutConstraint] = []
    func attach(_ view: UIView) {
        guard hosted !== view else { return }
        NSLayoutConstraint.deactivate(pins)
        hosted?.removeFromSuperview()
        hosted = view
        view.removeFromSuperview()
        view.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(view)
        pins = [view.leadingAnchor.constraint(equalTo: contentView.leadingAnchor), view.trailingAnchor.constraint(equalTo: contentView.trailingAnchor), view.topAnchor.constraint(equalTo: contentView.topAnchor), view.bottomAnchor.constraint(equalTo: contentView.bottomAnchor)]
        NSLayoutConstraint.activate(pins)
    }
    override func prepareForReuse() {
        super.prepareForReuse()
        NSLayoutConstraint.deactivate(pins); pins = []
        // Another cell may already own this retained view.
        if hosted?.superview === contentView { hosted?.removeFromSuperview() }
        hosted = nil
    }
}
#endif
