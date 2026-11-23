import MusterCore
import SwiftUI

/// Edits the local daemon's policy draft. Rust validates and applies every rule.
struct LocalServiceGrantsView: View {
    @Binding var policy: Muster_V1_Policy
    let members: [Muster_V1_MemberView]
    let advanced: Bool
    @State private var editing: Service?

    var body: some View {
        Section("Access to this device") {
            LabeledContent("Offer network", value: networkOffer)
            if !policy.hasOfferNetwork || !policy.offerNetwork {
                Button("Enable network service") { policy.offerNetwork = true }
            }
            if !policy.hasOfferNetwork || policy.offerNetwork {
                Button("Disable network service") { policy.offerNetwork = false }
            }
            ForEach(Service.allCases) { service in
                Button { editing = service } label: {
                    LabeledContent(service.title, value: summary(grants(for: service), for: service))
                }
            }
        }
        .sheet(item: $editing) { service in
            ServiceGrantEditor(
                service: service, existing: grants(for: service), members: members, advanced: advanced
            ) { setGrants($0, for: service) }
        }
    }

    private var networkOffer: String {
        guard policy.hasOfferNetwork else { return "Existing port policy" }
        return policy.offerNetwork ? "On" : "Off"
    }

    private func grants(for service: Service) -> Muster_V1_GrantSet? {
        let grants = policy.serviceGrants
        switch service {
        case .agents: return grants.hasAgents ? grants.agents : nil
        case .terminals: return grants.hasTerminals ? grants.terminals : nil
        case .network: return grants.hasNetwork ? grants.network : nil
        }
    }

    private func setGrants(_ value: Muster_V1_GrantSet?, for service: Service) {
        var grants = policy.serviceGrants
        switch service {
        case .agents:
            if let value { grants.agents = value } else { grants.clearAgents() }
        case .terminals:
            if let value { grants.terminals = value } else { grants.clearTerminals() }
        case .network:
            if let value { grants.network = value } else { grants.clearNetwork() }
        }
        policy.serviceGrants = grants
    }

    private func summary(_ grants: Muster_V1_GrantSet?, for service: Service) -> String {
        guard let grants else { return "Existing policy" }
        if grants.allDevices { return "All paired devices" }
        let visible = grants.rules.filter { service != .network || advanced || $0.scope == service.wholeScope }
        let assignments = visible.compactMap { rule -> String? in
            let names = members.filter { rule.deviceIds.contains($0.deviceID) }.map(\.name)
            guard !names.isEmpty else { return nil }
            let selector = rule.scope == service.wholeScope ? "All" : rule.scope
            return "\(selector) → \(names.joined(separator: ", "))"
        }
        return assignments.isEmpty ? "No assigned devices" : assignments.joined(separator: "\n")
    }
}

private enum Service: String, Identifiable, CaseIterable {
    case agents, terminals, network

    var id: Self { self }
    var title: String { rawValue.capitalized }
    var wholeScope: String { self == .network ? "net:*" : rawValue }
}

private struct ServiceGrantEditor: View {
    enum Audience: String, Identifiable, CaseIterable {
        case existing = "Existing policy"
        case all = "All paired devices"
        case assigned = "Assigned devices"

        var id: Self { self }
    }

    let service: Service
    let members: [Muster_V1_MemberView]
    let advanced: Bool
    let save: (Muster_V1_GrantSet?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var audience: Audience
    @State private var rules: [Muster_V1_DeviceGrant]

    init(
        service: Service, existing: Muster_V1_GrantSet?, members: [Muster_V1_MemberView],
        advanced: Bool, save: @escaping (Muster_V1_GrantSet?) -> Void
    ) {
        self.service = service
        self.members = members
        self.advanced = advanced
        self.save = save
        _audience = State(initialValue: existing.map { $0.allDevices ? .all : .assigned } ?? .existing)
        _rules = State(initialValue: existing?.rules ?? [])
    }

    private var visibleRules: [Int] {
        rules.indices.filter { service != .network || advanced || rules[$0].scope == service.wholeScope }
    }

    var body: some View {
        NavigationStack {
            Form {
                Picker("Allow access for", selection: $audience) {
                    ForEach(Audience.allCases) { option in
                        Text(option.rawValue).tag(option)
                    }
                }
                if audience == .assigned {
                    ForEach(visibleRules, id: \.self) { index in
                        Section("Rule") {
                            selector(index)
                            ForEach(members, id: \.deviceID) { member in
                                Toggle(member.name, isOn: assignment(member.deviceID, rule: index))
                            }
                            Button("Remove rule", role: .destructive) { rules.remove(at: index) }
                        }
                    }
                    Button("Add rule") {
                        rules.append(Muster_V1_DeviceGrant.with { $0.scope = service.wholeScope })
                    }
                }
            }
            .navigationTitle(service.title)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        save(audience == .existing ? nil : Muster_V1_GrantSet.with {
                            $0.allDevices = audience == .all
                            $0.rules = rules
                        })
                        dismiss()
                    }
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 440, minHeight: 420)
        #endif
    }

    @ViewBuilder
    private func selector(_ index: Int) -> some View {
        switch service {
        case .terminals:
            Picker("Permission", selection: $rules[index].scope) {
                Text("All terminal access").tag("terminals")
                Text("View").tag("terminal.view")
                Text("Control").tag("terminal.control")
            }
        case .network where !advanced:
            Text("Whole network")
        case .network:
            selectorField("Permission (net:* or net:tcp/443)", index)
        case .agents:
            selectorField("Permission (agents or agents:codex)", index)
        }
    }

    @ViewBuilder
    private func selectorField(_ title: String, _ index: Int) -> some View {
        #if os(iOS)
        MusterTextField(title, text: $rules[index].scope, monospace: true)
        #else
        TextField(title, text: $rules[index].scope).autocorrectionDisabled()
        #endif
    }

    private func assignment(_ device: String, rule index: Int) -> Binding<Bool> {
        Binding(
            get: { rules[index].deviceIds.contains(device) },
            set: { on in
                rules[index].deviceIds.removeAll { $0 == device }
                if on { rules[index].deviceIds.append(device) }
            }
        )
    }
}
