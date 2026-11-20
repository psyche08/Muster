# Windows 开发调试指南（面向 Agent）

## 在独立进程中操作窗口并截图

调试 Muster 自己的 WinForms 表单时，优先运行仓库的控件测试。测试进程加载实际的
`Muster.dll`，构造合成的 Shared Rust 视图投影，显示表单，操作控件并检查生成的
protobuf `Intent`。此流程可在普通用户权限下运行，开发机上的 GUI、daemon 和 vnet
可继续工作。它无需 Computer Use、全局鼠标键盘输入或向运行中的进程注入 DLL。

在 Windows x64 用 PowerShell 从仓库根目录运行。准备 .NET 10 SDK、protoc 和项目的
NuGet 依赖。开发工具和截图目录可以放在用户可写的开发目录。

```powershell
$project = 'src/native/tests/windows-agent-catalog/WindowsAgentCatalog.csproj'
$captures = Join-Path $env:TEMP 'Muster-form-captures'
dotnet run --project $project -c Release -- --screenshots $captures
```

测试生成 `desktop-session-empty.png`、`desktop-session-filled.png`，并打印实际
DPI 和所加载的程序集路径。图中仅包含合成设备和输入。省略 `--screenshots` 会执行
相同断言，跳过图片写入。

测试按控件的 `Name` 找到目标，选择设备和应用，填写目录和首条消息，然后在测试
进程内调用 `PerformClick()`。检查 `ShowDialog()` 返回值与请求的设备、目录、
提供方和消息。另一个用例经由 WinForms 的按键处理路径验证 Escape 取消。提供方
接收、远端权限、硬件签名和实际隧道流量需要分别验收。

截图使用 `Form.DrawToBitmap`。在 `Shown` 之后等待一次 UI 消息循环再取图，此时
Windows 已按实际 DPI 完成布局。在空白状态和可提交状态各取一张，检查控件边界、
多行输入可见行数、滚动区域与按钮。一次 192 DPI 验收中，按钮状态和请求都正确，
但截图和边界测量发现首条消息输入框只有 1 像素高。回归测试现在检查各行互不重叠，
且消息框至少能显示三行文字。`DrawToBitmap` 记录 WinForms 控件渲染。桌面合成器、
受保护系统窗口或其他应用的画面不在这项测试的覆盖范围内。

源码和 Git 差异预览可用同样的方法验收。以下命令覆盖文本、二进制、错误、截断、
Unicode 差异和历史改动入口，并保存当前文件及历史差异截图。

```powershell
dotnet run --project src/native/tests/windows-worktree/WindowsWorktree.csproj -c Release -- --screenshots $captures
```

同一项目还会操作对话页的 **Work steps** 原生菜单。实机验收时，选择一段已有工具输出的
测试对话。按轮次和用户提示找到对应记录，展开步骤、勾选工具查看输出，再折叠该轮次。
切换设备后核对展开状态。读取失败时，展开的轮次应显示宿主错误，可用 **Retry loading steps**
重试。普通消息输入框应保留未发送的草稿。验收只需读取已有记录。

终端控件测试在独立窗口中绘制带样式的行和光标，检查鼠标选区、普通字符、
补充平面字符、输入法提交消息、组合键、粘贴和尺寸上报。它可以输出一张合成截图。

```powershell
dotnet run --project src/native/tests/windows-terminal/WindowsTerminal.csproj -c Release -- --screenshots $captures
```

实机验收时在 Terminals 选择本机，新建专用的 Built-in terminal。在网格内输入
算式，先检查回显，再用方向键编辑并按 Return 确认计算结果。调整窗口大小后核对
目录行的列数和行数。Ctrl+Shift+V 粘贴，拖选后 Ctrl+Shift+C 复制，Ctrl+Tab
将焦点移回其他控件。关闭视图会保留 PowerShell，完成后在自己的测试会话中输入
`exit` 并确认它从目录移除。物理输入法候选窗、远端终端权限需另外验证。

对话内终端也由上述 WindowsTerminal 项目验收。`conversation-terminal.png` 是模拟已连接
宿主的窗格。另做端到端验收时，从 Agents 打开有工作目录的测试会话，先在输入框留一段
未发送的草稿，再点 **Workspace terminal**。核对目录、连接状态和回到 **Conversation**
后的草稿。**Disconnect** 分离视图；**Terminal actions…** 中的重启与结束会改变宿主上的
测试 shell。可在支持 tmux 且已授权的宿主上验证分离后重连到同一 shell。Windows 目标
当前会返回 `tmux requires a Unix host`。记录目标返回的错误以及相应操作是否仍可用。

## 与已交付的 ZIP 对照

如需证明截图来自特定包，在相同源码提交上给测试构建传入发行版本，例如
`dotnet run --project $project -c Release -p:Version=1.1.57 -- --screenshots $captures`。
把测试打印的 `Muster.dll` 绝对路径和发行 ZIP 的绝对路径分别赋给 `$assembly`、
`$package`。当前自包含 GUI 包保留嵌入程序集的原始字节，可以执行以下只读检查。

```powershell
python -c "import sys,zipfile; from pathlib import Path; dll=Path(sys.argv[1]).read_bytes(); z=zipfile.ZipFile(sys.argv[2]); sys.exit(0 if dll in z.read('payload/Muster.exe') else 1)" $assembly $package
if ($LASTEXITCODE) { throw '截图程序集与发行包不匹配' }
Get-FileHash -Algorithm SHA256 -LiteralPath $package
```

不匹配时核对源码提交、配置、发行版本与构建输出。记录实际测试的版本、提交、
包哈希、DPI、截图路径和通过的断言。完整 ZIP 构建、普通用户更新、固定快捷方式、
连接在线节点及隔离 daemon 的操作见 [Windows README](../src/native/windows/README.md)。

## 在线窗口与人工确认

需要核对真实窗口、焦点或单实例激活时，先确认进程路径、版本及所属用户，再定位
该进程的窗口和当前模态对话框。Windows UI Automation 的公开控件接口适用于实机
检查。使用 Computer Use 时遵守工具的会话规则。若父窗口禁用、子窗口未暴露，
重新获取窗口树，避免把输入投到背后的主窗。物理 Esc 可能终止 Computer Use。
此时保留在线窗口，通过独立控件测试继续可重复的验收。

普通应用更新、采集测试截图与 UI 测试都无需 UAC。Wintun 启动授权、TPM 在场确认
或账户登录由用户在系统窗口完成。验证聊天优先只读重开已有测试会话，新增真实
消息发送应提前说明用途。公开文档和版本库只使用合成截图、路径和设备记录。

## 复核桌面对话历史

在普通用户终端运行 `muster chats ChatGPT`，用当前目录中的完整会话 ID
运行 `muster chat-open ChatGPT <ID>`，再用返回的 ID 运行
`muster messages <ID> --tail 5`。只读验收可直接比对原有测试消息和回复。
`chat-open` 会切换提供方的桌面窗口。先核对所选择的是测试对话。

已有对话在新 daemon 中还没有被窗口观察到时，历史读取会提示从 Chats
目录重新打开。新建空白会话的零条消息是正常结果。旧标题回执和当前目录
ID 可以不同。如果提供方自己的历史仍能读取某条对话，而本机类型目录里
没有对应 ID，分别记录这两个结果。仅凭目录缺项不能判断消息已删除。
