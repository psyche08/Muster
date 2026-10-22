// The session's working tree: a listing, one file, the git log or a diff.
#if canImport(SwiftUI)
import MusterCore
import SwiftUI

struct FilesView: View {
    let core: MusterCore
    let vm: ViewModel

    private var files: Muster_App_FilesView { vm.files }

    var body: some View {
        List {
            if files.loading { ProgressView() }
            if !files.path.isEmpty {
                Button { core.openFiles(path: parent(of: files.path)) } label: { Label("..", systemImage: "arrow.turn.left.up") }
            }
            ForEach(files.entries, id: \.name) { entry in
                Button {
                    let path = files.path.isEmpty ? entry.name : files.path + "/" + entry.name
                    if entry.dir { core.openFiles(path: path) } else { core.openFile(path: path) }
                } label: {
                    HStack {
                        Image(systemName: entry.dir ? "folder" : "doc.text").foregroundStyle(entry.dir ? Color.accentColor : Color.secondary)
                        Text(entry.name)
                        Spacer()
                        if !entry.dir { Text(bytes(entry.size)).font(.footnote).foregroundStyle(.secondary) }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .navigationTitle(files.path.isEmpty ? "Files" : files.path)
    }

    private func parent(of path: String) -> String {
        guard let slash = path.lastIndex(of: "/") else { return "" }
        return String(path[..<slash])
    }
}

func bytes(_ n: UInt64) -> String {
    ByteCountFormatter.string(fromByteCount: Int64(n), countStyle: .file)
}

struct FileView: View {
    let core: MusterCore
    let vm: ViewModel

    private var file: Muster_App_FileView { vm.file }

    var body: some View {
        Group {
            if file.loading {
                ProgressView().padding()
            } else if !file.error.isEmpty {
                ContentUnavailableView("Preview unavailable", systemImage: "doc.questionmark", description: Text(file.error))
            } else if let text = String(data: file.content, encoding: .utf8) {
                SourcePreview(text: text, diff: file.isDiff)
            } else {
                ContentUnavailableView("Binary file", systemImage: "doc", description: Text("\(bytes(UInt64(file.content.count))) of binary data"))
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            VStack(alignment: .leading, spacing: 4) {
                Text(file.isDiff ? "Recorded turn changes" : "Current file contents").font(.caption).foregroundStyle(.secondary)
                if file.truncated { Text("Preview truncated").font(.caption).foregroundStyle(.orange) }
            }.frame(maxWidth: .infinity, alignment: .leading).padding(10).background(.bar)
        }
        .navigationTitle(file.path)
        .toolbar {
            if file.isDiff && !file.currentPath.isEmpty {
                Button("Current file") { core.openFile(path: file.currentPath) }
            }
        }
    }
}

struct SourcePreview: View {
    let text: String
    let diff: Bool
    private var lines: [String] { text.components(separatedBy: "\n") }

    var body: some View {
        ScrollView([.vertical, .horizontal]) {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(Array(lines.enumerated()), id: \.offset) { index, line in
                    HStack(alignment: .top, spacing: 12) {
                        if !diff {
                            Text("\(index + 1)").foregroundStyle(.tertiary).frame(minWidth: 32, alignment: .trailing)
                        }
                        Text(line.isEmpty ? " " : line).textSelection(.enabled)
                            .fixedSize(horizontal: true, vertical: false)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .padding(.horizontal, 12).padding(.vertical, 2)
                    .foregroundStyle(color(line))
                    .background(color(line).opacity(diff ? 0.08 : 0))
                }
            }
            .font(.system(.caption, design: .monospaced)).fixedSize(horizontal: true, vertical: false)
            .padding(.vertical, 8)
        }
    }
    private func color(_ line: String) -> Color {
        guard diff else { return .primary }
        if line.hasPrefix("+") && !line.hasPrefix("+++") { return .green }
        if line.hasPrefix("-") && !line.hasPrefix("---") { return .red }
        if line.hasPrefix("@@") { return .blue }
        return .primary
    }
}

struct GitView: View {
    let core: MusterCore
    let vm: ViewModel

    private var git: Muster_App_GitView { vm.git }

    var body: some View {
        Group {
            if git.loading {
                ProgressView()
            } else if !git.diff.isEmpty {
                SourcePreview(text: git.diff, diff: true)
            } else {
                List(git.commits, id: \.hash) { commit in
                    Button { core.openGit(diffCommit: commit.hash) } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(commit.subject).font(.headline).lineLimit(2)
                            Text("\(String(commit.hash.prefix(8))) · \(commit.author) · \(relative(commit.tsUnix))")
                                .font(.footnote).foregroundStyle(.secondary)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .navigationTitle(git.diffCommit.isEmpty ? "Git log" : String(git.diffCommit.prefix(8)))
    }
}
#endif
