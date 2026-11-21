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

## 免 UAC 演练两个版本的节点升级

要检查旧节点退出后，新节点能否读回相同的身份、地址和成员记录，使用
`windows_managed_terminal` 测试程序里的软件身份节点。普通 Windows 终端即可运行，
在线 GUI 和 daemon 可以继续工作。生产 `musterd.exe` 即使传入另一个 `--dir`，
仍会获取当前用户的 TPM 身份锁。

准备两个发行版本各自的干净源码工作树和便携包 CLI。将路径分别赋给
`$previousTree`、`$nextTree`、`$previousCli` 和 `$nextCli`。两个工作树均需包含
`fixture_node_process` 测试入口。两代可以共用 Cargo 下载缓存，每棵源码树使用独立的
`CARGO_TARGET_DIR`。混用编译产物曾导致新版节点链接到旧版工作区 crate，报告缺失接口。
下面将测试程序和编译产物放在用户可写的开发目录。

```powershell
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$fixtures = Join-Path 'D:\Developer\Muster\gui-check' ('node-upgrade-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($fixtures) | Out-Null
function Build-NodeFixture($tree, $cli, $name) {
    $version = & $cli --version
    if ($version -notmatch '^muster (\d+\.\d+\.\d+)$') { throw 'Expected a release CLI' }
    $saved = $env:MUSTER_WINDOWS_VERSION
    $savedTarget = $env:CARGO_TARGET_DIR
    try {
        $env:MUSTER_WINDOWS_VERSION = $Matches[1]
        $env:CARGO_TARGET_DIR = Join-Path $fixtures "target-$name"
        $manifest = Join-Path $tree 'src/muster/Cargo.toml'
        $exe = @(cargo test --manifest-path $manifest --locked -p musterd --test windows_managed_terminal --no-run --message-format=json-render-diagnostics |
            ForEach-Object { ConvertFrom-Json $_ } |
            Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'windows_managed_terminal' -and $_.executable } |
            ForEach-Object executable)
        if ($exe.Count -ne 1) { throw 'Expected one fixture binary' }
        (Copy-Item -LiteralPath $exe[0] -Destination (Join-Path $fixtures "$name.exe") -PassThru).FullName
    } finally {
        $env:MUSTER_WINDOWS_VERSION = $saved
        $env:CARGO_TARGET_DIR = $savedTarget
    }
}
$oldNode = Build-NodeFixture $previousTree $previousCli 'previous'
$newNode = Build-NodeFixture $nextTree $nextCli 'next'
./src/native/tests/windows-node-upgrade.ps1 -PreviousFixture $oldNode -NextFixture $newNode -PreviousCli $previousCli -NextCli $nextCli
```

脚本用旧版创建合成网络，保存设备名称和回环 relay，等旧进程正常退出，再用新版打开同一数据库。
建网后，脚本向旧节点注册一个临时软件 P-256 在场候选公钥。新旧节点返回的 SEC1 字节都应
与生成的公钥完全一致，且都报告未 enrollment。
两版 CLI 都会从两代节点读取设备信息。脚本比较身份、指纹、网络 ID、虚拟地址、角色和权限，
退出时删除临时数据库和软件密钥。记录测试输出的版本和提交，和两棵源码树的 HEAD 分别核对。
构建的两个测试程序保留在 `$fixtures`。生产切换后的节点版本、TPM 验签和实际隧道流量仍需分别验收。

正式切换前在 **Settings** 记下本机指纹、虚拟地址和 `Local node commit`。正常关闭 GUI，
待新 daemon 就绪后从固定快捷方式重新打开。核对同一指纹、新的 `Local node version` 和
`Local node commit`，再用发行包 CLI 的 `presence --summary` 查看候选注册和 enrollment
两行结果。1.1.89 的隔离演练中，保持旧窗口并点 **Refresh** 仍显示旧节点版本；重新打开
快捷方式后才读到新版提交。最后分别核对 Wintun 运行状态和对端的实际隧道流量。

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

在含多文件 diff 的合成 CLI 对话中，核对正文里的文件顺序及增删计数。通过
**File diffs…** 展开单个文件，核对旧行号、新行号和截断提示。**Copy diff** 使用该文件
完整的已提供补丁。下面的控件测试用内存回调逐字比对换行和预览之外的内容，保留系统剪贴板。

源码和 Git 差异预览可用同样的方法验收。以下命令覆盖文本、二进制、错误、截断、
Unicode 差异和历史改动入口，并保存当前文件及历史差异截图。

```powershell
dotnet run --project src/native/tests/windows-worktree/WindowsWorktree.csproj -c Release -- --screenshots $captures
```

同一用例生成 `related-sessions.png`。从 Agents 打开父会话，分别选择 **Subagents**
和 **Sessions**。核对标题、状态、关系和只读标记；空列表仍可进入。用鼠标或 Enter 打开
子会话，再点 **Open parent conversation** 回到父会话。使用相同标题的合成子项检查实际打开的
记录，切换设备后再核对同 ID 的目标。在窄窗口下检查标签可横向滚动。

在 **Chats** 目录打开支持模型选择的合成会话，点 **Model and effort**。用 UI Automation
按进程和所有者定位同名对话框，读取当前推理强度和选项表。点 **Refresh** 后等宿主返回，
核对选项说明和当前标记，再点 **Done** 返回同一会话。打开窗口和刷新都会查询宿主，
可能打开提供方的选择器。需要验证 **Apply choice** 时使用隔离节点；它会修改宿主上的设置。
验证前后台切换时，打开选项窗应能读到结果。让另一个测试进程的窗口取得焦点后，
向已核对的隔离选项窗请求刷新，预期 Shared Rust 取消后台读取。回到选项窗后再次刷新，
应能恢复宿主结果。记录这三个状态，可区分真正切换应用和仅打开应用内弹窗。
独立 WinForms 测试还覆盖模型与推理选项同名、错误后重试和会话宿主切换。

同一命令也生成 `session-trace.png`。在会话里点 **Trace**，按轮次查看已加载的记录。
可通过搜索命中详情，选择时间条或树中的记录核对开始时间、结束时间及原始输出。
在等宽与实际时间模式间切换；没有时间戳的记录仍可从树中查看。需要旧记录时点
**Earlier messages**，随后点 **Conversation** 回到同一对话。此流程只读取历史。

打开长对话时应能看到末条消息。在隔离节点的合成会话中追加一条多行记录，核对末行仍在视口。
向上滚动，记下第一条可见文字及其所属消息，再追加一条记录并加载较早的一页。两次更新后
该文字应留在原处。原生控件测试还会插入文字相同的消息，检查选区仍属于同一消息块。

在长对话里向上滚动，点 **Jump to latest**，核对末行进入视口，按钮消失。用
**Earlier messages** 退回较早的历史页，再点 **Jump to latest**；当前会话应一次回到
最新页。普通文本和含图片的会话分别验证，切到另一设备的会话后检查滚动位置。
此操作只读取历史，无需发送消息。

同一命令生成 `history-images.png`。合成对话含红、蓝两张 JPEG 和一份无效图片。
测试在实际渲染的像素中检查色带出现在对应文字之前，再通过含 120 张图片的历史
验证滚动到末尾和回到开头。切换会话时检查旧图清除。无需上传图片或发送消息。

在已有代码块的对话里点 **Copy code**。只有一个可见代码块时会直接复制；有多个时，
菜单按显示顺序列出语言与内容摘要。控件测试用合成消息核对完整源码，包含缩进、
空行、Unicode 和末尾换行。测试在内存中接收复制内容，可保留开发机的剪贴板。

同一项目还会操作对话页的 **Work steps** 原生菜单。实机验收时，选择一段已有工具输出的
测试对话。按轮次和用户提示找到对应记录，展开步骤、勾选工具查看输出，再折叠该轮次。
切换设备后核对展开状态。读取失败时，展开的轮次应显示宿主错误，可用 **Retry loading steps**
重试。普通消息输入框应保留未发送的草稿。验收只需读取已有记录。

打开有工作目录的旧 Windows 会话时，普通历史阅读可用。支持读取执行记录的新版节点
在 `muster info` 的服务列表中上报 `terminal.runs.v1`。验证读取 RPC 和重启恢复时，
从仓库根目录运行下面的隔离用例。它使用临时节点和合成记录，核对退出码及回传状态。

```powershell
cargo test --locked --manifest-path src/muster/Cargo.toml -p musterd --test windows_managed_terminal windows_host_reads_scoped_run_history_after_restart
```

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

通过 GUI 验收时，在合成对话输入未发送草稿，打开 **Terminal** 标签。先点击网格
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

临时路径较深时，还应运行 `managed_shell_in_deep_windows_temp`。它把子进程的
Windows TEMP 和 TMP 指向含中文和空格的深目录，然后在重启前后核对同一 shell。
tmux 的套接字地址最多 107 字节；新版节点会尝试随包 cygpath 给出的绝对 MSYS2 路径。
如果转换后仍超长，使用更短的开发节点目录。运行记录仍写在原节点目录。

此测试以软件身份启动两个先后运行的普通用户节点进程。它通过真实命名管道发现合成
会话、创建 shell、核对重复请求的回执并读取首帧。第一个节点退出后，第二个节点
应能从同一 shell 读取变量。显式重启会话后，原句柄应失效。测试自行结束专用
shell。含运行时的完整发行包、在线节点切换、远端授权和真实 GUI 重连需分别验收。

### 将 Windows 会话终端接到另一个 PowerShell 窗口

从 **Agents** 打开合成对话，选择 **Terminal**。标签页会尝试连接会话；手动断开后可点 **Connect**。
看到 **Disconnect** 表示已连接。打开 **Terminal actions… → Copy desktop connection command**。
在**运行该会话的 Windows 设备**打开一个普通用户 PowerShell，粘贴并运行命令。
Muster 和新窗口应能看到同一条命令的结果。外接窗口使用宿主返回的会话代际，
不会改变 Muster 的终端尺寸。关闭外接窗口可继续使用原 shell；显式重启会话后，
旧命令应退出。旧版 Windows 宿主可能未提供连接命令。

回归测试使用发行包的 MSYS2、系统 Windows PowerShell 和独立 ConPTY。将
`$env:MUSTER_MSYS2_ROOT` 指向展开的便携包内的 `payload\msys64` 或已安装版本的
`msys64`，然后运行：

```powershell
cargo test --locked --manifest-path src/muster/Cargo.toml -p muster-terminal powershell_desktop_client -- --ignored
```

测试在接入前保留半条合成命令，接入后补完并核对两端的输出，再从第二个窗口输入。
仅看到两个客户端已连接不能证明输入完整。此测试使用临时专用 socket，在线节点可继续运行。

用独立 PowerShell 窗口验收时，在原终端输入半条合成命令，接入后再补齐并回车。
例如先输入 `printf '%s%s\n' VIEW`，接入后输入 ` -SHARED`。在新窗口核对单独一行
`VIEW-SHARED`，再从新窗口运行同样的命令，把两段参数改为 `RETURN` 和 `-SHARED`，
回到原终端核对 `RETURN-SHARED`。显式重启这个合成会话后，再运行旧连接命令，核对
新一代会话没有增加外接客户端。

只需验证菜单时可保留系统剪贴板。用 UI Automation 枚举测试 GUI 进程自己的可见顶层
窗口，在弹出菜单里按 `ControlType.MenuItem` 和完整名称定位复制命令，检查其启用状态。
保持菜单展开，正常关闭测试主窗并记录进程退出码，预期为 0。上面的 Windows 终端控件
测试另行验证用 Enter 选中菜单项时回调恰好执行一次。

验收时把三种初始状态分开记录：从未启动过 shell、恢复已有 shell、结束最后一个
shell 后再启动。旧版宿主在第三种情况下曾报告 `tmux new-session timed out`。
先核对运行中的 daemon 版本。将 `MUSTER_MSYS2_ROOT` 指向便携包的 `msys64` 后，
从仓库根目录运行下列回归。它结束最后一个合成 shell，一次启动下一代，核对旧句柄
失效，并从新 shell 读回计算结果。

```powershell
cargo test --locked --manifest-path src/muster/Cargo.toml -p muster-terminal reopen_after_ending_the_last_shell -- --ignored
```

从 GUI 复核时只选择一次 **Terminal**，等待连接完成后在新 shell 输入合成命令并
核对输出。通过原始 RPC 验收可沿用 CLI 默认的 30 秒请求时限；手动设置过短的
时限可能在宿主完成前取消请求。

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

在 **Settings** 逐项核对本机网络。用同一发行包的 CLI 运行以下只读请求：

```powershell
'{"device_info":{}}' | & $musterCli rpc --request -
```

**Local relay** 和 **Relay registration** 分别对应 `relay` 和 `state.relay`。
**Assigned virtual IP** 对应 `state.vip`，**Local vnet** 对应 `state.vnet_up`。
已有虚拟地址时隧道仍可能显示 down。验证 **Change local relay** 时，用本文的隔离窗口
修改端点，并用带 `--socket $fixtureSocket` 的 CLI 复查。未加入网络的隔离节点刷新后会回到引导页，再打开 **Settings** 核对完整状态。
在线窗口可只打开后点 Cancel。
公开记录只需注明各项是否一致。

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

## 验收网络邀请

在隔离测试网络的管理员窗口中打开 **Settings → Create invite**。应能看到二维码、
可选择的邀请原文和倒计时。点 **New invite** 后核对新二维码对应新文本。以下命令
会从控件实际绘制的像素解码合成邀请，核对复制内容、更新、过期显示和离开页面后的清理。
测试将复制内容留在内存中，保留系统剪贴板。

```powershell
dotnet run --project src/native/tests/windows-agent-catalog/WindowsAgentCatalog.csproj -c Release
```

加入端在未加入网络时点 **Join with invite**。输入框为空时会打开粘贴对话框。
先取消并核对节点仍未加入，再用格式错误的合成邀请检查失败详情和 **Try again**。
跨节点验收用两台不同软件身份的临时节点和回环 relay。在第二个节点粘贴有效邀请，
由第一个节点核对加入者指纹并批准。两端应报告同一网络 ID 和一致的成员记录。
记录默认有效期、节点数量和是否成功即可，邀请原文和二维码无需写入日志。

## 核对网络设备详情

从 **Network** 或 **Settings** 单击设备行，也可用方向键选择后按 Enter。打开本机和
一个已配对的远端成员，分别核对名称、虚拟地址、指纹及上报的版本。链路取自该成员的
`link`，有值时原样显示。点 **Back** 返回设备列表。两个成员同名时，用指纹核对打开的
对象。**Access to other devices** 显示 Shared Rust 返回的分组摘要；只读核对无需保存权限。
桌面和服务器的设备详情可进入对应宿主的 Chrome、Agents 和 Terminals。核对没有服务的
成员时，这三个设备专属入口应隐藏。

## 复核 Chrome 扩展设置

从本机设备详情打开 **Chrome on device**。在普通用户终端读回同一节点的状态：

```powershell
'{"browser_setup":{"install":false}}' | & $musterCli rpc --request -
```

`installed` 表示原生宿主注册可用，`connected` 表示扩展实际回复了探测。分别记录两项。
本机的 **Show extension folder** 打开节点上报的目录。点 **Copy extensions address**，
在要配置的 Chrome 配置文件中将 `chrome://extensions/` 粘贴到地址栏，按 Enter。
也可通过 Chrome 菜单中的 **Extensions → Manage extensions** 进入。打开 **Developer mode**，
选择 **Load unpacked** 并选中 Muster 显示的目录，再回到 Muster 点 **Check connection**。
便携包的 `Apply-Update.cmd` 负责注册原生宿主。Chrome 会保存手动选择的解压目录。
复核升级时分别记录已注册的宿主路径和测试配置中扩展的来源目录。

验收导航时使用独立的临时 Chrome 配置。通过 UI Automation 核对该窗口的地址栏和
页面标题 **Extensions**。本机 Chrome 158 在首次启动和已有窗口两种情况下，接收
命令行地址后都停在新标签页；从地址栏输入相同地址后进入了扩展管理页。
检查启动参数只能证明应用发出了请求。

点 **Connection test** 后，编辑框应显示 ping 请求；点 **Run** 后再核对返回的能力和版本。
验收只需 ping。切换到远端设备时，核对本机目录和复制地址按钮已隐藏，执行按钮提示需要认证。
用临时软件节点和独立 Chrome 测试配置验收时，将浏览器子进程的 `MUSTER_DIR` 指向该节点
的数据目录，以便 Chrome 启动的原生宿主连接同一测试环境。

## 复核多设备 Agents 目录

在 **Agents** 中用设备筛选器复核出错的宿主，再回到所有设备。对已配对的目标，
先用 `muster status` 查看连接状态。如需检查订阅，把已安装的 CLI 路径和目标
设备 ID 分别赋给 `$musterCli` 和 `$deviceId`。在普通用户 PowerShell 中运行以下
Python 3 探针。它只请求目录，在内存中统计快照，输出观察时长和结束原因。

```powershell
@'
import json, subprocess, sys, time
cli, member = sys.argv[1:]
request = {"agent_watch": {"catalog_only": True, "provider_pages": True, "grouped": True, "limit": 10}}
start = time.monotonic()
result = subprocess.run([cli, "--target", member, "rpc", "--request", "-", "--timeout", "15"],
                        input=json.dumps(request).encode(), capture_output=True, timeout=30)
snapshots = 0
errors = [result.stderr.decode("utf-8", errors="replace")]
for line in result.stdout.splitlines():
    record = json.loads(line)
    event = record.get("event", {}).get("agent_event", {}).get("ev", {})
    snapshots += int("list_changed" in event)
    if "error" in record:
        errors.append(record["error"])
reason = " ".join(errors).lower()
if "unauthorized" in reason:
    end = "permission refused"
elif "rpc timed out and was cancelled" in reason:
    end = "observer deadline"
elif result.returncode == 0:
    end = "peer finished"
else:
    end = f"RPC error (exit {result.returncode})"
print(f"snapshots={snapshots} elapsed={time.monotonic()-start:.1f}s end={end}")
'@ | python - $musterCli $deviceId
```

`snapshots` 大于零且 `end=observer deadline` 表示收到了该成员的目录，订阅持续到
本次观察结束。快照为零且到达观察时限，表示窗口内未收到首个目录。
`end=permission refused` 表示对端明确拒绝访问。用 Agents 的设备筛选器选择该成员，
核对页面显示的具体权限，再返回所有设备。一台设备持续推送时，其他设备的错误应保留到
对应设备成功恢复或移出当前筛选。

用 UI Automation 核对此流程时，在 GUI 进程的可见顶层窗口中，按标题 `Muster` 和
`ControlType.Window` 找主窗，再按 `ControlType.MenuItem` 找设备选项。菜单已展开时，
`MainWindowHandle` 可能指向弹出菜单。Agents 错误正文在只读输出框的
`ValuePattern.Current.Value` 中。选中允许访问的单台设备后，应出现该设备的目录行；
回到 **All devices** 后，核对被拒成员的错误仍然可见。

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

目录有条目但提示显示主窗口时，从 Windows 开始菜单打开官方 ChatGPT，让一个主窗口
可见，然后再次运行 `muster chats ChatGPT`。提供方留在托盘时，进程和本地目录都可能
仍在；目录行数不能证明窗口可用。探测成功后若提示切换 Chat 模式，再从当前目录选择
已确认的测试对话。只验证窗口恢复时，无需切换模式或打开任何对话。

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
