# lfd：检查 macOS 卸载残留

[English](README.md)

`lfd` 是只读终端工具，用于查看应用的 `.app` 是否仍在，以及检查可能的关联文件。在 macOS 交互式终端中，从工作区根目录运行：

```sh
cargo run --locked --offline -p lfd
```

输入应用名称或精确的 Bundle ID，然后按 `Enter`。`lfd` 不接受命令行参数，也不接受路径作为查询。若名称匹配多个应用包，使用 `↑`/`↓` 和 `Enter` 选择一个；只有文件名匹配的项目不会归入其中任一身份。

使用 `↑`/`↓` 浏览左侧结果，右侧显示完整路径、匹配证据、类型和风险。`PgUp`/`PgDn` 滚动证据。若有无法检查的位置，按 `i` 单独查看路径和错误。结果页按 `q` 或 `Esc` 退出；输入页按 `Esc` 退出。

- **关联项**（Related items）：在已检查的应用目录中找到匹配的应用包。
- **残留候选**（Leftover candidates）：在已检查的应用目录中未找到匹配的应用包。这不能证明应用已卸载，也不能证明文件可以安全移除。
- **可能匹配**（Possible matches）：未找到匹配的应用包，但应用检查未完成。仅用户数据位置检查失败不会改变应用检查状态。

## 查找范围：应用与路径名称

| 检查位置 | `lfd` 检查内容 |
| --- | --- |
| `/Applications`、`/Applications/Utilities`、`/System/Applications`、`/System/Applications/Utilities`、`~/Applications` | 直接子项中的 `.app` 目录。匹配去掉 `.app` 的目录名，或读取 `Contents/Info.plist`（最多 4 MiB）中的 `CFBundleDisplayName`、`CFBundleName`、`CFBundleIdentifier`。使用可读字段；缺少元数据时仍可按目录名匹配。不验证或启动应用的可执行文件。 |
| `~/Library/Application Support`、`~/Library/Containers`、`~/Library/Group Containers`、`~/Library/Caches`、`~/Library/Logs` | 仅匹配直接子项的名称。 |
| `~/Library/Preferences`、`~/Library/Saved Application State` | 仅匹配直接子项的名称，分别去掉 `.plist` 和 `.savedState` 后匹配。 |
| `~/.config`、`~/.local/share`、`~/.local/state`、`~/.cache` | 仅匹配直接子项的名称。 |
| macOS 当前用户的专用临时目录（`getconf DARWIN_USER_TEMP_DIR`） | 仅匹配直接子项的名称。不使用 `$TMPDIR`，也不查找系统共用的临时目录。 |

应用名称匹配不区分大小写；元数据中的 Bundle ID 必须精确匹配，包括大小写。若元数据中有精确匹配的 ID，则优先于名称匹配。文件可按本机元数据中的 Bundle ID 精确匹配，也可按文件名匹配查询。形如 Bundle ID 的点分查询要求精确匹配文件名。仅凭用户输入的标识符不能证明归属。没有匹配元数据的路径名称命中仍只是文件名匹配。

按路径名称查找时，不进入用户数据目录的子目录。匹配的用户数据符号链接可出现在结果中，但不检查链接目标。当前版本不检查独立命令、Homebrew 记录或 `.pkg` 收据，也不查找整个主目录、浏览器数据或 Shell 历史。

## 查找范围：共享引用内容

除上文列出的应用包元数据外，`lfd` 仅读取以下来源的文件内容：

| 来源 | 检查的文件 |
| --- | --- |
| OpenCode | 仅 `~/.config/opencode/cli.json` 和 `~/.config/opencode/tui.jsonc`。 |
| Pi | `~/.pi/agent/extensions` 下扩展名为 `.js` 或 `.ts` 的直接子项普通文件。不进入嵌套扩展目录，也不读取其他类型的文件。 |

`lfd` 在 UTF-8 文本中按词边界做字面匹配：应用名称不区分大小写，Bundle ID 必须精确匹配。不解析或执行这些文件。匹配也可能来自注释或偶然出现的文本，不一定是生效的引用。证据栏展示路径和匹配词，不展示文件正文。引用不能证明该文件归应用专用，因此结果仅供查看。

`lfd` 不沿引用文件及其父目录中的符号链接进入目标。缺失的来源视为没有匹配；无法读取的文件、超过 1 MiB 的文件和其他检查失败单独报告，不视为检查成功但未找到匹配。

## 清理范围预览

普通构建中所有项目仅供查看：只查找应用目录，不足以完成清理前所需的主程序检查。名称匹配、用户输入的标识符、单独的收据、共享引用或单独的符号链接都不能使项目符合勾选条件。

在隔离预览验证中，只有必要的主程序检查已完成且未发现主程序，并有独立的 Bundle ID 登记证据时，才能勾选以下当前用户的精确路径：

- `~/Library/Application Support/<bundle-id>`（目录）及其直接包含的普通文件和目录。只检查名称和类型，不读取内容，也不跟随符号链接。
- `~/Library/Preferences/<bundle-id>.plist`（普通文件）。
- `~/Library/Caches/<bundle-id>`（目录）。

默认没有勾选项。按 `Space` 勾选或取消勾选合格项，按 `Enter` 预览已选路径。预览列出完整路径和已知风险（包括保留的数据或设置），说明目录包含其所有子项；如果同时勾选父目录和其中的子项，只在预览中列出父目录。按 `Enter` 确认默认的**取消**，或按 `Esc`，返回结果并保留勾选。此版本不能移动、修改或删除文件。
