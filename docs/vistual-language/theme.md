# GDS 共享主题与材质

本文定义开发中的主题来源、语义配色、材质与热更新接口。Files、Agents 和公共组件使用同一套视觉规则，分别保存应用偏好。页面组织见 [设置中心](../settings/README.md)，组件语义见 [视觉语言](README.md)。

## 1. 配置与数据流

外观设置只影响当前应用。来源读取、语义映射、Kit 桥接和界面消费依次执行：

```text
Noctalia / DMS 导出 / KDE 当前配色 / GNOME Portal 与 GTK 配置 / 内置与自定义 TOML
                                  ↓
                   provider：色板、实际明暗、依赖路径
                                  ↓
                    Palette / Colors 完整主题快照
                                  ↓
                     GPUI Kit 与 GDS 公共组件
```

应用读取语义角色，不复制来源解析或私有配色表。外部主题只提供颜色；布局、图标语义和交互由 GDS 定义。加载只读配置，不执行 QML/QSS、触发 Matugen 或修改桌面文件。

## 2. 来源与明暗

主题下拉提供「跟随系统 / 内置 / 自定义」。系统来源在 KDE Plasma 会话中读取当前 KDE 配色，GNOME 会话通过纯 Rust 读取桌面 Portal 明暗与强调色、Adwaita 基准配色及用户 GTK 3/4 配置；独立合成器会话按 `$XDG_RUNTIME_DIR` 中当前存活的 IPC Socket 识别正在运行的 Noctalia v5 或 DankMaterialShell，未检测到受支持桌面时不读取残留缓存文件。

Files defaults to the system palette and follows the desktop light/dark preference. 明暗还可单独选择浅色或深色。系统主题按当前会话与活跃 Shell 选择来源，已有 `dms` 配置兼容读取为 `system`。`ModePreference` 表示系统 / 浅 / 深，实际 `Mode` 只有浅 / 深。切换来源不重置明暗、字体、内置配色或表面不透明度。

### Noctalia v5

在非 KDE / 非 GNOME 会话中，当 `$XDG_RUNTIME_DIR/noctalia-${WAYLAND_DISPLAY:-wayland-0}.sock`（未注入 `WAYLAND_DISPLAY` 时扫描 `noctalia-*.sock`）可连接时识别为 Noctalia v5，读取 `$XDG_CACHE_HOME/gds/theme/noctalia.json`（默认 `~/.cache/gds/theme/noctalia.json`）。

软件包随附模板 `share/<binary>/noctalia/gds.json`（源码位于 [gds.json](../../src/appearance/theme/provider/system/noctalia/assets/gds.json)）。用户在 `~/.config/noctalia/gds.toml`（Noctalia 会自动合并配置目录下所有 `*.toml`）中登记模板后由 Noctalia 渲染输出：

```toml
[theme.templates.user.gds]
input_path = "/usr/share/gds-files/noctalia/gds.json"
output_path = "~/.cache/gds/theme/noctalia.json"
```

Nix / Home Manager 环境可将 `input_path` 指向 `"${config.programs.gds-files.package}/share/gds-files/noctalia/gds.json"`（或 `gds-agents` 包下对应路径）。模板导出 `schema_version = 1`、活动应用明暗 `mode`、`colors.{dark,light}` 的 Material 3 语义角色以及可选的 `terminal.{dark,light}` 22 项终端配色。跟随系统时使用导出的 `mode`，显式浅色／深色同时切换 UI 与终端的对应组。`terminal` 段整体缺失时回退到公共终端色表；一旦存在则原子校验全部角色与 16 色 ANSI 数组，任一缺失或非法均拒绝整次更新。实现见 [Noctalia provider](../../src/appearance/theme/provider/system/noctalia.rs)。

### DMS

在未检测到存活 Noctalia Socket 且 `DMS_SOCKET` 或 `$XDG_RUNTIME_DIR/{dms,danklinux}*.sock` 可连接时识别为 DMS，读取 `$XDG_CACHE_HOME/DankMaterialShell/dms-colors.json`，默认 `~/.cache/DankMaterialShell/dms-colors.json`。解析器读取 `colors.light` / `colors.dark` 的颜色角色；跟随系统时要求 `mode` 为 `light` 或 `dark`，显式明暗只读取对应组。

请求的组缺失、必需角色缺失、颜色非法或文件不可读均返回错误。可选表面角色从同一来源中补齐；未提供的业务状态色使用公共默认值。实现见 [DMS provider](../../src/appearance/theme/provider/system/dms.rs)。

### GNOME

在进程内通过纯 Rust 读取 XDG Settings Portal（`org.freedesktop.appearance` 的 `color-scheme` 与 `accent-color`）、标准 Adwaita 浅/深色基准色板，以及用户 `$XDG_CONFIG_HOME/gtk-3.0/`、`$XDG_CONFIG_HOME/gtk-4.0/` 内的 `settings.ini` 与 `gtk.css` `@define-color` 覆盖（`gtk-4.0` 优先级高于 `gtk-3.0`）。不再启动 Python 子进程或依赖 PyGObject / GTK 3 运行时库。

明暗在“跟随系统”模式下依次参考 Portal `color-scheme`、`settings.ini` 的 `gtk-application-prefer-dark-theme` 与平台窗口外观，也可在设置中显式选择浅色或深色。强调色优先采用用户 `gtk.css` 的 `@define-color` 覆盖，其次使用 Portal `accent-color` 并自动计算对比前景色，最后回落到 Adwaita 蓝色。监听 dconf 用户库、Settings Portal 信号及 GTK 3/4 的 `gtk.css`、`settings.ini`，变化后后台刷新。这里解析的是基准 Adwaita 语义与 `@define-color` 变量，不执行完整的第三方 GTK CSS 选择器引擎。

### KDE Plasma

当 `XDG_CURRENT_DESKTOP` 的冒号分隔列表包含 `KDE` 时，读取 `$XDG_CONFIG_HOME/kdeglobals`，默认 `~/.config/kdeglobals`。导入当前 `Colors:Window`、`Colors:View`、`Colors:Selection`、`Colors:Button` 与 `Colors:Tooltip`，不根据方案名重新读取原始 `.colors` 文件，因此保留 Plasma 应用后的强调色、染色和用户调整。Plasma 的写入逻辑见 [applyScheme](https://invent.kde.org/plasma/plasma-workspace/-/blob/master/kcms/colors/colorsapplicator.cpp)。

跟随系统时，从活动窗口背景推导明暗，与 KDE Settings Portal 的判断一致。Plasma 只导出一套活动配色，显式浅色／深色只覆盖 GDS 模式和缺失角色的默认值，不会生成另一套 KDE 配色；要完整跟随，请使用“系统”明暗。KConfig 的嵌套组独立解析，不混入非活动窗口颜色。配置缺失或颜色无效时使用既有错误处理，不切到残留的 DMS 导出。实现见 [KDE provider](../../src/appearance/theme/provider/system/kde.rs)。

## 3. 加载与刷新

启动时使用应用保存的来源；无法加载时明确报告并使用内置回退。运行中加载失败保留最后有效主题，不把失败表示为成功跟随。来源切换后的旧加载结果不能覆盖新选择。

`theme::Observer` 为每个进程统一监听 Noctalia 与 DMS 导出、Noctalia IPC Socket 启停、KDE `kdeglobals`、GNOME Portal/dconf/GTK 配置及自定义 TOML 主题文件。父目录监听支持原子替换，缺失路径创建后重新定位，托管符号链接同时监听目标。窗口只转发平台外观事件，不各自建立监听线程。

读取和解析在后台完成，成功后一次安装完整快照。事件源合并尚未消费的变化，不使用定时轮询或额外防抖。透明度变化只更新材质 alpha，不重新读取色板；字体与其它应用偏好保持独立。

## 4. 固定配色分工

Files、Agents 与公共组件共用 `src/appearance/theme/palette.rs` 的角色快照和 Kit 桥接。Noctalia、DMS / Matugen、GNOME、KDE 和内置／自定义来源只提供角色的值；应用不另设配色表。常规组件读取 Kit 的语义字段，自绘面板及状态层通过 `cx.gds().appearance().theme().colors()` 读取同一快照。主题热更新一次替换整份快照，透明度设置只改变材质 alpha。

来源实现按输入职责集中在 `theme/provider`：

```text
theme/
├── provider.rs              # Source、来源分派、加载结果与共同文件读取
├── provider/
│   ├── system.rs + system/  # 按桌面会话与活跃 Shell Socket 选择系统来源
│   │   ├── material.rs      # Noctalia 与 DMS 共用的 Material 3 语义映射
│   │   ├── noctalia.rs      # 解析 Noctalia v5 模板渲染的 UI 与终端配色
│   │   ├── noctalia/assets/ # 随包发布的 gds.json Noctalia 模板
│   │   ├── dms.rs           # 跟随 DMS 导出的活动方案与 dank16 终端色
│   │   ├── kde.rs           # 读取 KDE Plasma 当前 kdeglobals 配色
│   │   └── gnome.rs         # Portal、Adwaita 基准与用户 GTK 3/4 覆盖
│   ├── custom.rs            # 用户自定义 TOML 主题目录与解析
│   ├── builtin.rs           # 内置主题目录、嵌入与配置解析
│   ├── builtin/assets/      # 各套内置主题的 TOML 明暗配色
│   └── color.rs             # 文件来源共用的颜色语法解析
├── palette.rs               # 与来源无关的颜色角色和 GPUI Kit 映射
├── observer.rs              # 文件及平台外观变化通知
└── hot_reload.rs            # 加载、监听和安装的协调
```

provider 决定输入文件、解析方式与明暗来源，统一返回色板、实际模式和已读取色板路径。`system` 的 Noctalia 与 DMS 模式来自导出中的活动方案，KDE 模式来自当前配色，GNOME 结合 Portal、GTK 配置与平台窗口外观；`custom` 与 `builtin` 的系统模式来自平台明暗偏好。主题层负责应用色板及表面材质，热更新通过 provider 的依赖信息决定何时重新加载。

公开的 `theme::Source` 从 `provider::Source` 导出，保存值为 `system`、`builtin` 与 `{ custom = "..." }`；`dms` 是 `system` 的反序列化兼容别名，界面中统一展示为「跟随系统 / 内置 / 自定义」。

### 内置主题配置

内置配色使用 `provider/builtin/assets/*.toml`，通过 `include_str!` 编入可执行文件，首次使用时解析并缓存。每个文件包含完整的 `[light]` 和 `[dark]` 颜色角色，值采用不透明的 `#RRGGBB`；表面透明度仍由应用偏好单独控制。

| 主题 | 配色方向 | 配置文件 |
| --- | --- | --- |
| 竹林 / Bamboo | 清新绿色，默认内置配色 | [bamboo.toml](../../src/appearance/theme/provider/builtin/assets/bamboo.toml) |
| 怀旧 / Retro | Win98 式灰阶、海军蓝选中态与青绿 | [retro.toml](../../src/appearance/theme/provider/builtin/assets/retro.toml) |
| 霓虹 / Neon | 合成波紫、粉与青色 | [neon.toml](../../src/appearance/theme/provider/builtin/assets/neon.toml) |
| 马卡龙 / Macaron | 奶油白、玫瑰粉、薄荷绿与薰衣草紫 | [macaron.toml](../../src/appearance/theme/provider/builtin/assets/macaron.toml) |
| 高对比 / High Contrast | 黑白表面、清晰边界与醒目的蓝／黄强调色 | [high_contrast.toml](../../src/appearance/theme/provider/builtin/assets/high_contrast.toml) |
| Aero | Vista / Windows 7 风格的冰蓝表面与天蓝强调色 | [aero.toml](../../src/appearance/theme/provider/builtin/assets/aero.toml) |

`theme::BuiltinTheme` 提供主题目录，配置值为 `bamboo`、`retro`、`neon`、`macaron`、`high_contrast`、`aero`。Files 使用 `appearance.builtin`，Agents 使用 `appearance.builtin_theme`；旧配置省略此字段时使用竹林，原有系统来源选择保持原值。两个应用在内置来源下显示配色选择，选择立即应用并保存，可单独恢复为竹林；切换来源会保留内置配色选择。

Files 配置示例：

```toml
[appearance]
source = "builtin"
builtin = "retro"
mode = "system"
```

明暗选择独立于主题：跟随系统使用相应明暗版，手动浅色或深色覆盖平台模式。Noctalia / DMS / KDE / GNOME 中缺少的业务状态角色继续使用竹林的公共默认值。修改配色只需编辑对应 TOML 并重新构建；增加主题时同时登记 `BuiltinTheme` 目录和嵌入文件。

| GDS 语义 | KDE Plasma 输入 | Noctalia / DMS（Material 3）输入 | 说明 |
| --- | --- | --- | --- |
| `surface.window` | `Colors:Window` | surface | 窗口底色 |
| `surface.content` | `Colors:View` | surface | 文件内容 / 输入内容底色 |
| `surface.sidebar` | `Colors:Window` | surface_container_low | 缺失时回到 surface，并用分隔线保证层级 |
| `surface.control` | `Colors:Button` | surface_container | 普通控件背景 |
| `surface.panel` | `Colors:Button`（背景／前景） | surface_container_high / on_surface | Agents 遥控面板等抬升表面；对应 `Colors.panel` / `panel_foreground` |
| `surface.overlay` | `Colors:Tooltip`（背景／前景） | surface_container_high / on_surface | 菜单、提示、拖动预览与弹出层；对应 Kit `popover` / `popover_foreground`，保持不透明 |
| `text.primary` | `Colors:Window` / `Colors:View` `ForegroundNormal` | on_surface | 按所在表面选择成对的前景色 |
| `text.secondary` | `Colors:View.ForegroundInactive` | on_surface_variant | 检查可读性，不机械降低全控件 alpha |
| `accent` | `Colors:Selection`（背景／前景） | primary / on_primary | 强调，不等同所有选中背景 |
| `selection.bg` / `fg` | `Colors:Selection`（背景／前景） | primary_container / on_primary_container | 成对导入，不能强制白字 |
| `border.subtle` | `Colors:Window.ForegroundInactive` / 派生 | outline_variant | 优先保留表面区分度 |
| `border.control` | `Colors:Window.ForegroundInactive` / 派生 | outline | 输入框等需要辨认的边界；对应 Kit `input`，不替代细分隔线 |
| `focus` / `progress` | `Colors:Selection.BackgroundNormal` | primary | 焦点环、插入光标、进度和活动标记；不作为普通面板的染色 |
| `shadow` | 公共阴影色 | shadow | 仅用于投影；玻璃面板的边缘高光取自自身前景色 |
| `status.error` | 公共状态色补全 | error / on_error | KDE 标准颜色组没有完整业务状态语义 |
| `status.success` / `status.warning` | 公共状态色补全 | 公共状态色补全 | 成功、警告使用套件的明暗成对默认值；不把任意 secondary / tertiary 色解释成绿或黄 |
| disabled / inactive | 非活动 KConfig 子组不混入活动表面 | 基于语义角色生成 | 非活动窗口不等于禁用窗口 |

容器色与文字色成对使用。Files 的页签、文件行、卷行及容量文字，Agents 的会话、命令候选、附件、队列编辑项和书签标记，在选中时使用 `primary_container` / `on_primary_container`，不强制白字，也不保留选中前的次级文字色。正文用 `on_surface`，侧栏正文用所在来源的侧栏前景，分区标题、路径和辅助动作使用不透明的 `on_surface_variant`。

窗口根表面的透明度通过 Kit `RootPlugin::style` 设置，不清空全局 `background` 颜色或 token。原生 Dialog 继续读取不透明的主题背景；主内容、侧栏和表格继承各自材质，主窗口透明度不传递给对话框。

现有文本框和选择框采用描边式设计，统一通过 `ui::field::input` / `select` 构造：内部使用 `surface.content` 和正文前景，外框使用 `outline`，不从描边色推导底色。显式无外观的编辑器和嵌入式搜索框继续继承宿主表面。

中性悬停层统一取所在表面前景的 8%，按下层取 12%；有底色的按钮在主题层与各自的成对前景混合。拖放和框选填充统一取 `primary` 的 12%，轮廓使用 `primary`。持久选中使用完整的容器 / 前景配对，不能用按下色代替。公共 `ui::button::Button` 负责幽灵按钮的中性状态，避免 Kit 再自行调亮、调暗或在悬停时换成选中文字色；Files 与 Agents 不各自定义一组透明度。

玻璃遥控面板只取 `surface.panel`，采用统一半透明材质、圆角、边缘和投影；不额外混入白色、主色或大面积渐变。主内容和侧栏分别保持用户设置的不透明度，文字和图标不跟随表面透明度变淡。语法高亮、终端 ANSI 与内容图片沿用各自的内容配色，不用 UI 角色覆盖。

缺失角色由公共映射补齐：Noctalia 与 DMS 的 control 缺失时取导入的 content，panel 缺失时取 control，outline 缺失时取 subtle border；KDE 的 panel 使用 `Colors:Button`，overlay 使用 `Colors:Tooltip`。外部来源缺失层级时不掺入内置主题的表面色。外部主题的布局、圆角和交互不会透传。

## 5. 表面与平台能力

非平铺桌面（例如 KDE Plasma、GNOME）主界面默认 100% 不透明，侧栏默认 85%；Niri、Sway、Hyprland 等已识别平铺会话保留主界面 85%、侧栏 50%。默认值由 [公共 tokens](../../src/appearance/tokens.rs) 定义。应用提供两个独立百分比滑块，修改即时应用、保存并可逐项恢复默认。顶栏、底栏和终端左列沿用所属列的材质，每个区域只绘制一次背景；文字和图标保持不透明。

窗口允许 alpha，菜单、提示与原生对话框使用不透明表面。透明设置不保证桌面背板模糊；模糊是否可用取决于 GPUI 后端、窗口系统和合成器，不能作为通用 Linux 能力承诺。关闭材质效果后仍通过语义颜色、间距、边界和焦点保持层次。

## 6. 持久化与托管配置

Files 保存到 `$XDG_CONFIG_HOME/gds/app/files/settings.toml`，Agents 保存到自己的前端配置；不存在套件全局外观同步。应用真实变化后自动保存，失败通过共享通知报告。

声明式符号链接与只读文件允许读取，但不会被替换或覆盖。运行态修改已成功、写入失败时明确报告“已生效但未保存”。外部主题文件始终只读；GDS 不替用户安装 Noctalia、DMS 或修改会话环境。

## Window frames and initial size

Files and Agents start with a 960 × 640 logical-pixel workspace, capped at 85%
of the display work area on smaller screens. The compositor can override this
allocation when tiling. Files keeps a 520 × 360 minimum so its compact layout
remains usable.

Files has two responsive layouts, both retaining the page title. The expanded
layout shows the directory search field and all page actions. Below approximately
1080 px, the compact layout collapses the sidebar and hides back/forward/parent
controls. The title and search stay in the page header; the remaining actions
collapse into an ellipsis button at the right end of that same header. Sorting
and the application menu are available as submenus. The main menu button stays
in the top-left corner in both layouts, with the same contents and Alt+M / F10
bindings; it is independent of the page-actions ellipsis. Details are disabled in the
compact layout, including their shortcut. Expanding restores the saved sidebar
and details preferences. The breakpoint budgets for the expanded details panel,
even when it is currently closed. Long titles truncate within their available
space instead of disappearing at an independent breakpoint.

Both appearance pages offer **Rounded window corners**, disabled by default.
`appearance.rounded_corners` is saved independently by each application and applies
immediately. Linux client-side frames use the shared 10 px radius, including their
content clip and input region. Maximized/fullscreen windows and corners touching a
tiled edge stay square; server-side decorations remain desktop-owned.


## 用户主题与配置目录

共享用户主题位于 `$XDG_CONFIG_HOME/gds/themes/`（默认 `~/.config/gds/themes/`），Files 与 Agents 的配置分别位于 `gds/app/files/settings.toml` 和 `gds/app/agents/settings.json`。运行时只读写新配置路径，不查找或迁移历史路径。应用的 XDG state/cache 路径不受此调整影响。

用户主题复用内置主题的 TOML 格式，包含完整的 `[light]` 与 `[dark]` 配色角色，颜色为 `#RRGGBB`；可复制 `src/appearance/theme/provider/builtin/assets/bamboo.toml` 开始编辑。两款应用的主题来源为「跟随系统 / 内置 / 自定义」；选择自定义后，下方的独立菜单展示主题目录内的有效 TOML 主题（包括子目录）。菜单优先显示当前应用语言对应的译名，缺失或为空时回退到默认名称；用户选择保存的是相对主题目录的路径。新增文件后重新打开设置；编辑已选中的文件会触发热加载，非法配色保留最后有效主题并报告错误。用户主题的亮暗选择与表面透明度继续由各应用独立设置。

Files 配置示例：

```toml
[appearance]
source = { custom = "personal/my-theme.toml" }
mode = "system"
```

Agents 配置中的对应字段：

```json
{"appearance": {"theme_source": {"custom": "personal/my-theme.toml"}}}
```


### 自定义主题元信息

主题文件在 `[metadata]` 中声明默认名称；`name` 为唯一必填的元信息，不能为空或只含空白。`schema_version` 可省略（按当前版本 1 解释），显式指定未知版本会报错；`author` 与 `i18n` 均可省略。旧的无名称主题需补充 `[metadata]` 后使用。内置 TOML 示例已包含元信息，可直接复制。

```toml
[metadata]
name = "Forest"
schema_version = 1
author = "Theme Author"

[metadata.i18n]
zh-CN = "森林"
en-US = "Forest"

# Follow with the complete [light] and [dark] palettes.
```

`i18n` 是语言标记到主题名称的映射；按当前应用语言标记（`zh-CN`、`en-US`，忽略大小写）匹配，无匹配时直接回退到 `name`。不会借用其它语言的译名。无效文件不会进入选单；正在使用的主题变为无效时，热加载继续保留最后有效配色并报告错误。
