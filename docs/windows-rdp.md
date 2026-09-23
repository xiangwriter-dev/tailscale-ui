# Windows 远程桌面

适用于 xiangwriter远程器 0.2.1。用户已选择从软件打开 Windows 自带远程桌面；本轮只交付 Windows，macOS/Linux 开发与验收搁置。

## 已配置好后怎么连接

1. 两台电脑的官方 Tailscale 已登录，目标电脑已开启 Windows 远程桌面。
2. 安装 0.2.1 后打开“设备”。软件启动时自动读取设备，回到前台会再刷新，也可点击右上角刷新按钮。
3. 点击目标行的“远程桌面”；也可先选中设备，再从右侧详情连接。
4. 软件自动使用该目标最新的 Tailscale IP 打开系统远程桌面，在 Windows 窗口里完成账号登录、证书确认等操作。

原来的“配对执行端”用于运行命令和脚本。只连接 Windows 桌面无需安装或配对本应用的任务执行端，无需在软件内粘贴配对信息，也不需要手工输入目标 IP。

应用不会读取或保存 Windows 登录密码。系统可能继续显示登录或证书提示；应用里的“已打开”仅表示已启动客户端，不代表远端登录成功。

## 连接入口不可用

- 本机：请选择另一台 Windows 设备。
- 历史记录或网络失效：刷新设备，并确认两端仍在预期 Tailscale 网络中。
- 非 Windows 目标或非 Windows 控制端：当前入口未提供这些平台的客户端适配。
- 缺少 mstsc.exe：软件会显示系统组件不可用。
- 系统窗口无法连接：检查目标远程桌面设置、账户权限、防火墙与 Tailscale 策略。能 ping 通只说明网络可达，不证明远程桌面服务或账号已就绪。

连接功能调用 Windows 系统目录中的 mstsc.exe，传入目标地址，不自动提权、不关闭证书检查、不修改 Tailscale 配置或远端 RDP 设置。

参考：[Tailscale 官方 RDP 指南](https://tailscale.com/docs/solutions/access-remote-desktops-using-windows-rdp)、[Microsoft mstsc 参数说明](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/mstsc)。
