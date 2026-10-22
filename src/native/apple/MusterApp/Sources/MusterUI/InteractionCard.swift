#if canImport(SwiftUI)
import SwiftUI
import MusterCore

/// Draft selections are user input only; Shared Rust validates and submits them.
struct InteractionCard: View {
    let interaction: Muster_V1_PendingInteraction
    let submitting: Bool
    let error: String
    let outcomeUnknown: Bool
    let respond: (String, [Muster_V1_QuestionAnswer]) -> Void
    @State private var expanded = false
    @State private var selections: [String: Set<String>] = [:]
    @State private var custom: Set<String> = []
    @State private var text: [String: String] = [:]
    @Environment(\.conversationStyle) private var style
    @Environment(\.colorScheme) private var scheme
    private var palette: ChatPalette { ChatPalette(scheme: scheme, style: style) }

    var body: some View {
        if interaction.optional {
            DisclosureGroup(isExpanded: $expanded) {
                card.padding(.top, 10)
            } label: {
                Label("可选回答", systemImage: "bubble.left").font(.subheadline)
            }.tint(palette.secondary)
        } else {
            card
        }
    }

    private var card: some View {
        VStack(alignment: .leading, spacing: 18) {
            Label(interaction.optional ? "可选回答" : (interaction.kind == "question" ? "Your input is needed" : "Approval needed"),
                  systemImage: interaction.kind == "question" ? "bubble.left.and.text.bubble.right" : "hand.raised")
                .font(.headline)
            if !interaction.prompt.isEmpty {
                Text(interaction.prompt).textSelection(.enabled)
            }
            if interaction.questions.isEmpty {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(interaction.options, id: \.self) { option in
                        Button(option) { respond(option, []) }.buttonStyle(.bordered)
                    }
                }
            } else {
                ForEach(Array(interaction.questions.enumerated()), id: \.element.id) { index, question in
                    if index > 0 { Divider() }
                    VStack(alignment: .leading, spacing: 10) {
                        HStack {
                            Text("\(index + 1) / \(interaction.questions.count)")
                            if !question.header.isEmpty { Text(question.header) }
                            Spacer()
                            Text(question.multiSelect ? "Select all that apply" : "Select one")
                        }.font(.caption).foregroundStyle(palette.secondary)
                        Text(question.prompt).font(.body.weight(.medium)).textSelection(.enabled)
                        ForEach(question.options, id: \.id) { option in
                            let selected = selections[question.id, default: []].contains(option.id)
                            Button {
                                if question.multiSelect {
                                    if selected { selections[question.id, default: []].remove(option.id) }
                                    else { selections[question.id, default: []].insert(option.id) }
                                } else {
                                    selections[question.id] = [option.id]
                                    custom.remove(question.id)
                                }
                            } label: {
                                OptionRow(label: option.label, detail: option.description_p,
                                          multiSelect: question.multiSelect, selected: selected, palette: palette)
                            }.buttonStyle(.plain)
                                .accessibilityAddTraits(selected ? .isSelected : [])
                        }
                        if question.allowText {
                            if !question.options.isEmpty {
                                let selected = custom.contains(question.id)
                                Button {
                                    if selected { custom.remove(question.id) }
                                    else {
                                        custom.insert(question.id)
                                        if !question.multiSelect { selections[question.id] = [] }
                                    }
                                } label: {
                                    OptionRow(label: "Other", detail: "输入自己的回答",
                                              multiSelect: question.multiSelect, selected: selected, palette: palette)
                                }.buttonStyle(.plain)
                                    .accessibilityLabel("Other，自定义回答")
                                    .accessibilityAddTraits(selected ? .isSelected : [])
                            }
                            if custom.contains(question.id) || question.options.isEmpty {
                                answerField(for: question)
                                    .background(palette.canvas, in: RoundedRectangle(cornerRadius: 10))
                                    .overlay(RoundedRectangle(cornerRadius: 10).stroke(palette.secondary.opacity(0.25)))
                                    .accessibilityLabel(question.prompt)
                            }
                        }
                    }
                }
                Button {
                    let answers = interaction.questions.map { question in
                        Muster_V1_QuestionAnswer.with {
                            $0.questionID = question.id
                            $0.optionIds = question.options.filter { selections[question.id, default: []].contains($0.id) }.map(\.id)
                            $0.text = custom.contains(question.id) || question.options.isEmpty ? text[question.id, default: ""] : ""
                        }
                    }
                    respond("", answers)
                } label: {
                    HStack {
                        if submitting { ProgressView().controlSize(.small) }
                        Text(submitting ? "Submitting…" : "Submit answers")
                    }.frame(maxWidth: .infinity)
                }.buttonStyle(.borderedProminent).controlSize(.large)
            }
            if !error.isEmpty { Label(error, systemImage: outcomeUnknown ? "questionmark.circle" : "exclamationmark.circle").font(.callout).foregroundStyle(.red) }
            if outcomeUnknown { Text("Check the answer in the desktop app before trying again.").font(.caption).foregroundStyle(palette.secondary) }
        }
        .disabled(submitting || outcomeUnknown)
        .padding(18)
        .foregroundStyle(palette.text)
        .background(palette.surface, in: RoundedRectangle(cornerRadius: style.messageRadius))
        .overlay(RoundedRectangle(cornerRadius: style.messageRadius).stroke(palette.secondary.opacity(0.25)))
        .onChange(of: interaction.id) { _, _ in selections = [:]; custom = []; text = [:] }
    }
}
/// One choice of a question: a check box or radio, its label and detail.
extension InteractionCard {
    /// The free-text answer, with the Muster keyboard on iOS.
    @ViewBuilder
    fileprivate func answerField(for question: Muster_V1_InteractionQuestion) -> some View {
        let binding = Binding(get: { text[question.id, default: ""] }, set: { text[question.id] = $0 })
        #if os(iOS)
        MusterTextEditor("Your answer", text: binding, minimumHeight: 48, maximumHeight: 180).padding(4)
        #else
        TextField("Your answer", text: binding, axis: .vertical)
            .lineLimit(2...8).textFieldStyle(.plain).padding(12)
        #endif
    }
}

private struct OptionRow: View {
    let label: String
    let detail: String
    let multiSelect: Bool
    let selected: Bool
    let palette: ChatPalette

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: multiSelect ? (selected ? "checkmark.square.fill" : "square") : (selected ? "largecircle.fill.circle" : "circle"))
                .foregroundStyle(selected ? Color.accentColor : palette.secondary)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 4) {
                Text(label).font(.body.weight(selected ? .semibold : .regular))
                if !detail.isEmpty {
                    Text(detail).font(.callout).foregroundStyle(palette.secondary)
                }
            }
            Spacer(minLength: 0)
        }.frame(maxWidth: .infinity, alignment: .leading).padding(12)
            .background(selected ? Color.accentColor.opacity(0.09) : palette.canvas, in: RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(selected ? Color.accentColor : palette.secondary.opacity(0.2)))
            .contentShape(Rectangle())
    }
}
#endif
