# lfd：检查 macOS 卸载残留

[English](README.md)

`lfd` 是只读终端工具，用于查看应用的 `.app` 是否仍在，以及检查可能的关联文件。在 macOS 交互式终端中，从工作区根目录运行：

```sh
cargo run --locked --offline -p lfd
```

输入应用名称、精确的 Bundle ID 或精确的 `.pkg` 包标识符，然后按 `Enter`。`lfd` 不接受命令行参数，也不接受路径作为查询。若匹配多个应用身份，使用 `↑`/`↓` 和 `Enter` 选择一个；只有文件名匹配的项目不会归入任何身份。

使用 `↑`/`↓` 浏览左侧结果，右侧显示完整路径、匹配证据、类型和风险。`PgUp`/`PgDn` 滚动证据。若有无法检查的位置，按 `i` 单独查看路径和错误。结果页按 `q` 或 `Esc` 退出；输入页按 `Esc` 退出。

- **关联项**（Related items）：在已检查的应用目录中找到匹配的应用包。
- **残留候选**（Leftover candidates）：在已检查的应用目录中未找到匹配的应用包。这不能证明应用已卸载，也不能证明文件可以安全移除。
- **可能匹配**（Possible matches）：必要的应用检查未完成。仅数据位置检查失败或收据读取失败，不会改变应用检查状态。

## 初始查找范围

| 检查位置 | `lfd` 检查内容 |
| --- | --- |
| `/Applications`、`/Applications/Utilities`、`/System/Applications`、`/System/Applications/Utilities`、`~/Applications` | 直接子项中的 `.app` 目录。匹配去掉 `.app` 的目录名，或读取 `Contents/Info.plist`（最多 4 MiB）中的 `CFBundleDisplayName`、`CFBundleName`、`CFBundleIdentifier`。使用可读字段；缺少元数据时仍可按目录名匹配。不验证或启动应用的可执行文件。 |
| `~/Library/Application Support`、`~/Library/Preferences`、`~/Library/Caches` | 仅匹配直接子项的名称，偏好设置文件名可去掉 `.plist` 后匹配。不打开用户数据文件，也不进入匹配的目录。 |

应用名称匹配不区分大小写；元数据中的 Bundle ID 必须精确匹配，包括大小写。若元数据中有精确匹配的 ID，则优先于名称匹配。文件可按本机元数据中的 Bundle ID 精确匹配，也可按文件名匹配查询。形如 Bundle ID 的点分查询要求精确匹配文件名。用户输入的标识符没有独立证据时只能产生文件名匹配，不能证明归属。

不沿符号链接进入目标。匹配的用户数据链接可能出现在结果中，但不检查链接目标。缺失的查找目录视为没有条目；读取错误和元数据检查失败单独报告。

## 已登记的 `.pkg` 收据

`lfd` 读取 `/private/var/db/receipts` 下已登记的收据 plist（每个最多 4 MiB）。可按区分大小写的精确 `PackageIdentifier` 匹配，也可按收据记录的安装包文件名（去掉 `.pkg`）匹配，后者不区分大小写。如果安装包文件名只是重复包标识符，包标识符仍区分大小写。`lfd` 使用 macOS 的 `/usr/bin/lsbom` 读取匹配收据的 `.bom`，根据 `InstallPrefixPath` 确定记录的位置。不打开下载的安装包，不运行脚本。没有匹配收据时仍会执行其他查找。

只展示上述查找范围内当前仍存在的路径。若收据指向上述应用目录内的 `.app`，`lfd` 不沿符号链接读取当前的 `Contents/Info.plist`。可读的 Bundle ID 与包标识符相同，或当前元数据的大小和校验和与收据记录相符，才能印证该应用包。这不验证其可执行文件，也不证明当前归属。若旧路径现在有无法核实身份的应用，检查视为未完成；按 `i` 可查看路径及原因。记录中的路径缺失不代表其他位置也不存在。收据及仅凭收据匹配的路径只供查看。

此范围不包括独立命令、Homebrew 记录或共享配置内容，也不会查找整个主目录。没有可清理位置：`lfd` 不勾选、移动、修改或删除任何项目。
