# lfd：检查 macOS 卸载残留

[English](README.md)

`lfd` 是只读终端工具，用于查看应用的主程序是否仍在，以及检查可能的关联文件。在 macOS 交互式终端中，从工作区根目录运行：

```sh
cargo run --locked --offline -p lfd
```

输入应用名称、精确的 Bundle ID、命令名或精确的 Homebrew 包名，然后按 `Enter`。`lfd` 不接受命令行参数，也不接受路径作为查询。若匹配到可区分的应用、命令或 Homebrew 包，使用 `↑`/`↓` 和 `Enter` 选择一个；只有文件名匹配的项目不会归入所选身份。

使用 `↑`/`↓` 浏览左侧结果，右侧显示完整路径、匹配证据、类型和风险。`PgUp`/`PgDn` 滚动证据。若有无法检查的位置，按 `i` 单独查看路径和错误。结果页按 `q` 或 `Esc` 退出；输入页按 `Esc` 退出。

- **关联项**（Related items）：在已检查位置找到匹配的主程序。
- **残留候选**（Leftover candidates）：在已检查位置未找到匹配的主程序。这不能证明应用已卸载，也不能证明文件可以安全移除。
- **可能匹配**（Possible matches）：未找到匹配的主程序，但一项必要的主程序检查未完成。仅用户数据位置检查失败不会改变这个状态。

## 初始查找范围

| 检查位置 | `lfd` 检查内容 |
| --- | --- |
| `/Applications`、`/Applications/Utilities`、`/System/Applications`、`/System/Applications/Utilities`、`~/Applications` | 直接子项中的 `.app` 目录。匹配去掉 `.app` 的目录名，或读取 `Contents/Info.plist`（最多 4 MiB）中的 `CFBundleDisplayName`、`CFBundleName`、`CFBundleIdentifier`。使用可读字段；缺少元数据时仍可按目录名匹配。不验证或启动应用的可执行文件。 |
| `PATH` 中的目录 | 检查与查询命令名对应的条目。命令必须是设置了至少一个执行权限位的普通文件；不检查其内容，也不启动它。 |
| `/opt/homebrew` 和 `/usr/local` 下的 `Cellar/<formula>/<version>` 和 `Caskroom/<cask>` | 精确匹配包名。对 formula，读取 `INSTALL_RECEIPT.json` 并检查各版本 `bin` 和 `sbin` 目录的直接子项。缺少可用记录时，只报告同名可执行文件，名称并非归属证据。对 cask，读取 `.metadata/INSTALL_RECEIPT.json` 并检查应用目录中的 `app` 目标（默认 `/Applications`）；未找到时检查该版本在 Caskroom 的暂存应用包。对 `binary`，仅检查该版本目录内的源文件。每份记录最多读取 4 MiB，不检查其他类型的安装项或范围外路径。 |
| `~/Library/Application Support`、`~/Library/Preferences`、`~/Library/Caches` | 仅匹配直接子项的名称，偏好设置文件名可去掉 `.plist` 后匹配。不打开用户数据文件，也不进入匹配的目录。 |

应用名称匹配不区分大小写；元数据中的 Bundle ID 必须精确匹配，包括大小写。若元数据中有精确匹配的 ID，则优先于名称匹配。文件可按本机元数据中的 Bundle ID 精确匹配，也可按文件名匹配查询。形如 Bundle ID 的点分查询要求精确匹配文件名。用户输入的标识符没有独立证据时只能产生文件名匹配，不能证明归属。

匹配应用元数据中的精确 Bundle ID 时，主程序检查只针对应用目录。否则按名称检查应用目录和 `PATH`，并在标准 Homebrew 位置检查精确匹配的包名。Homebrew 记录可指向待检查的主程序，但记录本身不能证明主程序仍在。记录缺失时继续其他检查。Homebrew 的全局位置只供查看；`lfd` 不调用 Homebrew、不联网、不执行脚本。

不沿符号链接进入目标，包括 `PATH` 上的命令链接。匹配的用户数据链接可能出现在结果中，但不检查链接目标。缺失的查找目录视为没有条目；读取错误和元数据检查失败单独报告。

初始范围不包括 `.pkg` 收据或共享配置内容，也不会查找整个主目录。没有可清理位置：`lfd` 不勾选、移动、修改或删除任何项目。
