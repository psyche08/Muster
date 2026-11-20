# Windows 开发调试指南（面向 Agent）

## 普通用户联调 GUI、FFI 和 daemon

在仓库根目录用 PowerShell 7.4 或更高版本运行。开发工具和 Cargo 产物可放在
用户可写的开发目录。先关闭现有 Muster 主窗口，在线 daemon 和 vnet 可继续运行。

```powershell
./src/native/windows/dev.ps1 -Mode Isolated
```

脚本从同一工作树构建实际 GUI、Rust FFI、CLI 和使用软件身份的本地测试 daemon。
在 **Muster (isolated development)** 窗口操作；另开终端，把脚本打印的两个路径
分别赋给 `$musterCli` 和 `$fixtureSocket`，再运行：

```powershell
& $musterCli --socket $fixtureSocket info
& $musterCli --socket $fixtureSocket presence
```

核对节点名称为 `Development desktop`、指纹与在线节点不同、vnet 关闭。
始终传入打印的 socket；省略它会连接已安装节点。可在测试窗口中创建网络，
随后用同一个 socket 核对网络状态。关闭测试窗口会结束隔离 daemon 并丢弃临时数据。
重新打开固定快捷方式即可回到在线节点。若已有主窗口，隔离启动会退出并提示关闭
该窗口。客户端改动必须访问真实设备时仍然可以使用 `-Mode Client`。

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

在已有代码块的对话里点 **Copy code**。只有一个可见代码块时会直接复制；有多个时，
菜单按显示顺序列出语言与内容摘要。控件测试用合成消息核对完整源码，包含缩进、
空行、Unicode 和末尾换行。测试在内存中接收复制内容，可保留开发机的剪贴板。

同一项目还会操作对话页的 **Work steps** 原生菜单。实机验收时，选择一段已有工具输出的
测试对话。按轮次和用户提示找到对应记录，展开步骤、勾选工具查看输出，再折叠该轮次。
切换设备后核对展开状态。读取失败时，展开的轮次应显示宿主错误，可用 **Retry loading steps**
重试。普通消息输入框应保留未发送的草稿。验收只需读取已有记录。

终端控件测试在独立窗口中绘制带样式的行和光标，检查鼠标选区、普通字符、
补充平面字符、输入法提交消息、组合键、粘贴和尺寸上报。它可以输出一张合成截图。

```powershell
dotnet run --project src/native/tests/windows-terminal/WindowsTerminal.csproj -c Release -- --screenshots $captures
```

同一测试还会生成 `recent-runs.png`。在对话页点 **Recent runs** 可查看 Shared Rust
返回的执行记录。用例覆盖已知和未知退出码、截断输出、记录重排和移除、回传资格变化，
以及切换设备或对话。点击回传按钮时，测试只检查生成的 Intent；真实提供方回执需单独验收。

实机验收时在 Terminals 选择本机，新建专用的 Built-in terminal。在网格内输入
算式，先检查回显，再用方向键编辑并按 Return 确认计算结果。调整窗口大小后核对
目录行的列数和行数。Ctrl+Shift+V 粘贴，拖选后 Ctrl+Shift+C 复制，Ctrl+Tab
将焦点移回其他控件。关闭视图会保留 PowerShell，完成后在自己的测试会话中输入
`exit` 并确认它从目录移除。物理输入法候选窗、远端终端权限需另外验证。

对话内终端也由上述 WindowsTerminal 项目验收。`conversation-terminal.png` 是模拟已连接
宿主的窗格。另做端到端验收时，从 Agents 打开有工作目录的测试会话，先在输入框留一段
未发送的草稿，再点 **Workspace terminal**。核对目录、连接状态和回到 **Conversation**
后的草稿。**Disconnect** 分离视图；**Terminal actions…** 中的重启与结束会改变宿主上的
测试 shell。可在支持 tmux 且已授权的宿主上验证分离后重连到同一 shell。Windows 宿主需要包含托管终端支持的 musterd 和 MSYS2 tmux。旧节点仍可能报告 `tmux requires a Unix host`；先在 Settings 核对节点的版本和提交。

调试 Windows 宿主的 tmux 兼容性时，用专用测试 socket 创建两个合成会话。用
`display-message -p -t '=会话名:'` 读取 `#{session_name}` 和 `#{session_id}`，
核对名字和数字 ID。通过 ConPTY 输入一个算式，在返回的完整网格里读到计算结果，
再从 tmux 侧检查调整后的尺寸。关闭第一个测试进程，用第二个进程恢复同一 shell
中的变量。显式重启其中一个会话后，核对旧句柄失效，另一个会话仍可输入。

共享适配器通过操作系统设置 tmux 客户端的工作目录，再以 `-c .` 创建会话。
在本机已有 MSYS2 tmux 时，可在单独的普通用户 PowerShell 窗口运行下列回归测试。
将第一行换成实际运行时的绝对路径。测试使用该目录中的 `tmux.exe` 和 `cygpath.exe`。

```powershell
$env:MUSTER_TEST_MSYS2_BIN = 'D:\Developer\msys64\usr\bin'
$env:MSYS = 'noglob'
$env:CHERE_INVOKING = '1'
$env:TERM = 'xterm-256color'
$env:SHELL = '/usr/bin/sh'
cargo test --manifest-path src/muster/Cargo.toml -p muster-terminal --lib windows_tmux_preserves_each_native_working_directory -- --ignored
```

`noglob` 保留查询参数中的花括号，`CHERE_INVOKING` 让登录 shell 保留客户端的目录。
测试在临时 socket 上创建两个会话，从窗格读回工作目录，再与磁盘上的目录核对。
它覆盖普通盘符路径和带扩展前缀的规范路径，第二个目录名含中文、空格和
`#{session_id}`。重启到缺失目录应返回错误，原会话仍能恢复。关闭测试用的
PowerShell 窗口即可丢弃上述环境设置。正式 daemon 重启及 GUI 重连还需单独验收。

### 通过隔离 daemon 验证 Windows 托管终端

1.1.71 起，便携包在 `musterd.exe` 同级提供 `msys64`。正常运行时无需设置环境变量。
验收发行包时，可把测试生成的 `windows_managed_terminal-*.exe` 复制到临时目录，
在同级放入从 ZIP 解出的 `payload/msys64`。在没有 `MUSTER_MSYS2_ROOT` 的普通
PowerShell 会话中运行该测试程序，参数为
`--ignored --exact managed_shell_through_windows_daemon_restart`。只保留系统 PATH
可以验证 shell 能独立执行随包的 `cat`，在恢复后从原工作目录读回文件。
1.1.72 起还可运行 `--ignored --exact packaged_shell_shares_the_windows_user_temp_directory`，
验证 Windows 和 Bash 能在当前用户的临时目录中相互读写文件。

通过 GUI 验收时，在合成对话输入未发送草稿，打开 **Workspace terminal**。先点击网格
获得焦点，再用 `cat` 读取合成项目中的测试文件。自绘网格通过 MSAA 的名称
`Terminal` 和 `accValue` 提供画面文本。核对 **Disconnect → Connect** 后输出仍在，
并核对 **Conversation** 恢复原草稿。

在普通用户 PowerShell 中把 `MUSTER_MSYS2_ROOT` 设为可信 MSYS2 安装目录的绝对路径。
目录下应有 `usr\bin\tmux.exe`、`sh.exe`、运行库和终端数据。daemon 默认也会检查其
可执行文件旁的 `msys64`。该路径由宿主选择；工作目录里的程序不会作为 tmux 启动。

```powershell
$env:MUSTER_MSYS2_ROOT = 'D:\Developer\msys64'
cargo test --manifest-path src/muster/Cargo.toml -p musterd --test windows_managed_terminal managed_shell_through_windows_daemon_restart -- --ignored
```

此测试以软件身份启动两个先后运行的普通用户节点进程。它通过真实命名管道发现合成
会话、创建 shell、核对重复请求的回执并读取首帧。第一个节点退出后，第二个节点
应能从同一 shell 读取变量。显式重启会话后，原句柄应失效。测试自行结束专用
shell。含运行时的完整发行包、在线节点切换、远端授权和真实 GUI 重连需分别验收。

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

便携包更新后，从固定快捷方式打开 **Settings**。`Desktop version` 是当前 GUI 的版本；
`Local node version` 和 `Local node commit` 来自节点上报。用该包的
`cli\muster.exe info` 核对在线节点版本。若旧 daemon 继续提供网络服务，桌面和节点
可以显示不同版本。从 1.1.66 起，Windows 便携 CLI 的 `--version` 可离线报告
其自身发行版本；未打入发行版本的开发 CLI 会显示 crate 版本。在设备详情中，`Device version` 和 `Device commit` 对应当前
所选设备。字段显示 `not reported` 时，在验收记录中将其标为未上报。

## 在线窗口与人工确认

需要核对真实窗口、焦点或单实例激活时，先确认进程路径、版本及所属用户，再定位
该进程的窗口和当前模态对话框。Windows UI Automation 的公开控件接口适用于实机
检查。使用 Computer Use 时遵守工具的会话规则。若父窗口禁用、子窗口未暴露，
重新获取窗口树，避免把输入投到背后的主窗。物理 Esc 可能终止 Computer Use。
此时保留在线窗口，通过独立控件测试继续可重复的验收。

### 示例：不用 Computer Use 核对 New CLI session 的 Cancel

- **复现取消行为**：运行上面的 `WindowsAgentCatalog` 命令。它打开真实的 CLI 表单，在测试进程内走 WinForms 的 Escape 处理路径，断言 `ShowDialog()` 返回 `DialogResult.Cancel`。同次运行的两张 PNG 来自 **New desktop session**；CLI 取消的证据是断言。整个测试使用合成数据，普通用户即可运行。
- **操作已安装的 Muster**：在同一用户的交互桌面上，按 GUI 的进程 ID 查找标题为 **New CLI session** 的模态窗，核对它的所有者是当前 Muster 主窗。在该对话框内查找已启用的 **Cancel** 按钮，通过 UI Automation 的 `InvokePattern.Invoke()` 调用一次。重新查询窗口，确认对话框关闭且 **Agents** 主窗恢复可用。若控件树只有禁用的主窗，重新枚举该进程的顶层窗口；仍无法定位时，记录实机步骤未验证。此路径直接调用控件，无需模拟全局按键或注入 DLL。[Microsoft UI Automation](https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/invoke-a-control-using-ui-automation)
- **留存实机画面**：需要核对实际显示效果时，可使用支持 Windows Graphics Capture 的工具，通过系统窗口选择器选中目标窗口再取图。先确认预览包含目标对话框；独立模态窗可能需要单独选择。记录窗口标题、版本和 DPI，在线截图仅保存在本机。仓库的 `--screenshots` 参数用于独立表单测试。[Microsoft 窗口捕获文档](https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture)

普通应用更新、采集测试截图与 UI 测试都无需 UAC。Wintun 启动授权、TPM 在场确认
或账户登录由用户在系统窗口完成。验证聊天优先只读重开已有测试会话，新增真实
消息发送应提前说明用途。公开文档和版本库只使用合成截图、路径和设备记录。

## 复核多设备 Agents 目录

在 **Agents** 中用设备筛选器复核出错的宿主，再回到所有设备。对已配对的目标，
先用 `muster status` 查看连接状态。如需检查订阅，把已安装的 CLI 路径和目标
设备 ID 分别赋给 `$musterCli` 和 `$deviceId`。以下命令只请求目录，在六秒后取消观察。

```powershell
'{"agent_watch":{"catalog_only":true,"provider_pages":true,"grouped":true,"limit":10}}' |
    & $musterCli --target $deviceId rpc --request - --timeout 6
```

分别记录设备是否在线、是否收到首个目录快照、是否收到明确的权限拒绝。收到快照后，
CLI 仍可在观察时限结束时报告超时。记录结果时只需事件类型和计数，省略会话标题。
一台设备持续推送时，其他设备的错误应保留到对应设备成功恢复或移出当前筛选。

## 只读核对在场密钥

在已安装的 GUI 中打开 **Presence key**，用同一发行包的
`cli\muster.exe presence --summary` 核对两行结果。若使用隔离 daemon，给 CLI
显式传入开发脚本打印的 `--socket`。记录候选公钥注册状态和网络管理员 enrollment
状态。查询失败时记录错误。**Settings → Register presence candidate** 会访问 TPM
并注册候选公钥，只读验收使用前一个入口即可。

签名验收单独记录挑战、Windows 本人确认、对端验签以及改动请求 ID 后的拒绝结果。
每次验收标注日期和版本，区分历史验签记录与本次节点状态。

## 复核桌面对话历史

在普通用户终端运行 `muster chats ChatGPT`，用当前目录中的完整会话 ID
运行 `muster chat-open ChatGPT <ID>`，再用返回的 ID 运行
`muster messages <ID> --tail 5`。只读验收可直接比对原有测试消息和回复。
`chat-open` 会切换提供方的桌面窗口。先核对所选择的是测试对话。

已有对话在新 daemon 中还没有被窗口观察到时，历史读取会提示从 Chats
目录重新打开。新建空白会话的零条消息是正常结果。旧标题回执和当前目录
ID 可以不同。如果提供方自己的历史仍能读取某条对话，而本机类型目录里
没有对应 ID，分别记录这两个结果。仅凭目录缺项不能判断消息已删除。

按稳定 ID 核对提供方的**已归档**列表。归档的对话可以保有完整历史，同时不在
当前 Chats 目录中。读取它的历史、检查归档归属、从 Muster 目录重新打开，要分别
记录结果。不要仅按同名标题认定是原对话。保持只读验收时无需改变归档状态。

Windows 的 Chats 目录在打开对话后仍可从左侧访问。选择已列出的测试对话，可用
鼠标单击或方向键加 Enter。核对所选提供方、宿主、对话正文，以及切回后未发送的
草稿。目录只剩缓存且没有可用宿主时，保留列表供查看，打开操作应禁用。
