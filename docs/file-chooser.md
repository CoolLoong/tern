# 系统文件选择器

Tern 实现 XDG Desktop Portal 后端接口
[`org.freedesktop.impl.portal.FileChooser`](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.FileChooser.html)，
不是用 FileManager1 打开普通窗口来模拟选择文件。

- `OpenFile`：单选、多选、目录选择，返回所选项目的 `file://` URI。
- `SaveFile`：输入文件名；已有文件必须再次确认覆盖。选择器本身不写入目标文件。
- `SaveFiles`：选择文件夹，按调用方提供的文件名返回路径，避免已有文件冲突。
- glob/MIME 类型过滤、当前文件夹/名称/过滤器以及调用方提供的 choices。
- 搜索只匹配当前目录的直接子项，不递归查找子目录；文件类型和隐藏文件过滤同时生效。
- `org.freedesktop.impl.portal.Request.Close` 会取消对应请求并关闭窗口；窗口关闭也返回取消。
- MIME 类型通过文件名扩展名判断，不做内容嗅探。当前路径编辑和目录展示要求 UTF-8 路径。

后端以 `tern --portal-service` 单独驻留，D-Bus 名称是
`org.freedesktop.impl.portal.desktop.tern`。普通 Tern 实例和工作区不会接收这些请求。
协议处理位于 `src/platform/file_chooser.rs`，GPUI 对话框位于 `src/browser/chooser.rs`。

## Niri 中启用

安装包提供 `.portal` 描述文件、D-Bus 激活服务和专用窗口规则。
在 `~/.config/xdg-desktop-portal/portals.conf` 的 `[preferred]` 中增加：

```ini
org.freedesktop.impl.portal.FileChooser=tern;gtk;
```

保留其余接口的现有选择，尤其 ScreenCast、Screenshot 和默认后端。
在 Niri 配置中包含：

```kdl
include "/usr/share/tern/tern-file-chooser.kdl"
```

规则只匹配 `io.github.coolloong.Tern.FileChooser`，使用 `open-floating true`，
不会把普通文件管理窗口改成浮动窗口。参见
[Niri 浮动窗口说明](https://niri-wm.github.io/niri/Floating-Windows.html)。
对话框依然由 Niri 管理，但显示在浮动层，不创建平铺列。

配置生效需要重新启动 `xdg-desktop-portal.service` 或重新登录。
浏览器必须使用 portal 文件选择接口；绕过 portal 的原生 GTK 选择器不会被此配置替换。
必要时在浏览器自身的启动配置中启用 portal 并重启浏览器。

目前没有导入调用方的外部 Wayland `parent_window`，因此不能保证与浏览器建立真正的
父子模态关系；Niri 专用规则负责浮动、初始尺寸和聚焦。

## 无界面验证

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
export RUSTFLAGS='-C link-arg=-fuse-ld=lld'
env -u DISPLAY -u WAYLAND_DISPLAY -u NIRI_SOCKET -u DBUS_SESSION_BUS_ADDRESS \
  dbus-run-session -- cargo test --release --locked --bin tern chooser_tests -- --test-threads=1
```

测试覆盖协议返回、请求取消和调用者隔离、中文路径、过滤器/choices、覆盖确认、
模拟窗口浮动属性与关闭，以及浏览面板间距和双击路径编辑。测试不会打开真实窗口。
