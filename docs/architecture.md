# Tern 的场景模块

按使用场景组织应用代码，在场景内部区分 GPUI 界面、状态和服务。
`src/main.rs` 只声明场景模块并调用 `app::run()`；不在根模块重新导出旧的扁平模块名。

```text
src/
├── main.rs
├── app/           启动、GPUI 初始化、Action、快捷键、应用菜单、资源
├── workspace/     主窗口、Dock 布局、窗口管理、状态汇总、布局持久化
├── browser/       文件浏览场景：面板、View、TableDelegate、交互与操作适配
├── files/         文件模型、读取、排序、搜索、Recent、文件操作、取消机制
├── navigation/    Places 侧栏和书签
├── settings/      配置持久化、主题应用、设置窗口
└── platform/      CLI 路径解析、D-Bus 单实例、Niri 激活、外部应用
```

## GPUI 组合与状态归属

1. `app/bootstrap.rs` 处理命令行、单实例选主/转发及配置加载。
2. `app/runtime.rs` 创建 GPUI Application，初始化共享 Global，接收打开请求。
3. `workspace/windows.rs` 创建窗口；`workspace/window.rs` 组合 Dock、侧栏和浏览面板。
4. `browser/panel.rs` 是 Dock 适配层，持有 `browser/view/mod.rs` 的 Entity。
   `view/layout.rs` 负责界面组合，`view/operations.rs` 负责操作对话框与命令；
   `view/mod.rs` 保留状态、导航和订阅。
5. `browser/table/mod.rs` 持有当前目录、选择、查询和任务槽；文件处理交给后台执行器。
   `table/render.rs` 单独实现 GPUI TableDelegate 渲染，回归测试放在相邻测试模块。
6. `files/listing.rs` 返回不可变行快照和索引视图。前台回调先检查任务身份，再发布结果。

`workspace/state.rs` 管理共享窗口句柄，`workspace/status.rs` 保留现有状态栏
Global 的语义。浏览器的导航历史、选中项、加载和取消状态仍归各自 Entity 所有。
场景内部按职责拆分，状态仍由原有 Entity 持有。

## 模块边界

- `files/` 不依赖 GPUI 或其他应用场景。模型在 `model.rs`，取消原语在
  `jobs.rs`，因此 `fs.rs`、`recent.rs`、`search.rs` 无需反向依赖列表编排模块。
- `files/operations.rs` 只做文件操作。GPUI 任务调度和传输进度适配在
  `browser/transfers.rs`。
- 应用 Action 定义在 `app/actions.rs`，键位映射在 `app/keybindings.rs`。
  窗口命令路由在 `workspace/commands.rs`，浏览器右键菜单在
  `browser/context_menu.rs`，不再混用 `menu` 和 `menus`。
- 浏览面板的持久化结构和面板名归 `browser/panel.rs`，由 workspace 注册面板工厂。
- `settings/config.rs` 管配置数据和磁盘访问；`settings/theme.rs` 负责向 GPUI 应用主题；
  `settings/window.rs` 负责设置界面。
- 场景界面可以调用窗口命令、共享状态和应用 Action；这些交互没有包装成无意义的通用层。
  可复用控件继续放在现有 `crates/`，不为每个场景新建 Cargo crate。

## 兼容性与验证

Action 仍使用显式的 `tern` 命名空间；D-Bus 名称、`tern.browser_panel`、
配置字段、布局版本、文件路径和桌面 app_id 都保持原值。
原有六项失败已修复；验证不得排除这些测试。

```sh
python3 tools/check-architecture.py
./tools/check-headless.sh
```

结构检查约束源码根目录与 `files/` 的界面依赖；行为验证继续使用无真实窗口的
GPUI TestPlatform/TestWindow。统一脚本运行整个 Cargo workspace 与字体缓存测试，
使用独立会话 D-Bus，并移除显示器与 Niri 连接变量。它不代表真实 GPU 驱动、首次绘制
或浏览器集成在每个桌面环境中均已验收。
