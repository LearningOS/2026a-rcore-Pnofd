# 本地 QEMU 8.2.2 配置说明

课程要求 QEMU 8.2.x（GitHub 评测使用 8.2.2）。本机为 Ubuntu 22.04，情况如下：

- apt（jammy）最高只有 QEMU **6.2.0**，不满足要求；
- QEMU 8.2.2 的官方 deb 依赖 `libc6 (>= 2.38)`、`libglib2.0-0t64`，
  而 jammy 是 glibc 2.35，**deb 装不上**；
- `~/.bashrc` 第 143 行把 `/opt/qemu-4.1.1/bin` 放在 PATH 最前面，那是**另一个实验**要用的。

因此本项目**从源码编译 QEMU 8.2.2**，安装在项目内的 `.qemu/8.2.2/`，
并在 `os/Makefile` 中用绝对路径绑定，与 4.1.1 完全隔离。

## 实测对比（同一份 `os.bin`，ch3）

| QEMU | 版本 | 结果 |
| --- | --- | --- |
| 项目自带 `.qemu/8.2.2` | 8.2.2 | ✅ 全部测试通过，`All applications completed!` |
| `/opt/qemu-4.1.1` | 4.1.1 | ❌ 固件打印完即卡住，无内核输出 |
| `/usr/bin`（系统 apt） | 6.2.0 | ❌ 同上 |

结论：**4.1.1 和 6.2.0 都无法启动 RustSBI 0.4.1 固件**，必须用 8.2.2。

## 使用方式

直接 `make -C os run` 即可，Makefile 会自动选项目内的 8.2.2：

```sh
make -C os run
make -C os debug        # tmux + gdb，同样用 8.2.2
```

临时切换到其他版本（不影响默认行为）：

```sh
make -C os run QEMU=/opt/qemu-4.1.1/bin/qemu-system-riscv64
```

如果 `.qemu/8.2.2` 不存在，`QEMU` 会自动回退到 PATH 中的 `qemu-system-riscv64`。

## 另一个实验不受影响

`~/.bashrc` 中的两行**保持原样**，未做任何修改：

```sh
export PATH=$PATH:/home/a/Downloads/riscv64-unknown-elf-gcc-8.3.0-.../bin
export PATH=/opt/qemu-4.1.1/bin:$PATH
```

`/opt/qemu-4.1.1/` 目录本身也未改动。4.1.1 实验照常使用系统的 `qemu-system-riscv64`。

## 重新编译（若 `.qemu/` 被删除）

`.qemu/` 已加入 `.gitignore`（86MB 二进制，不进版本库）。重建步骤：

```sh
cd /home/a/Documents/2026fos/2026a-rcore-Pnofd
mkdir -p .qemu-build && cd .qemu-build

# 1. 源码（清华镜像，SHA256 与 dsc 清单一致）
curl -LO https://mirrors.tuna.tsinghua.edu.cn/ubuntu/pool/main/q/qemu/qemu_8.2.2+ds.orig.tar.xz
curl -LO https://mirrors.tuna.tsinghua.edu.cn/ubuntu/pool/main/q/qemu/qemu_8.2.2+ds-0ubuntu1.18.debian.tar.xz
tar -xf qemu_8.2.2+ds.orig.tar.xz
tar -xJf qemu_8.2.2+ds-0ubuntu1.18.debian.tar.xz -C qemu-8.2.2

# 2. 应用 Debian 补丁（共 130 个，去掉剔除掉的 bios blob 相关逻辑）
cd qemu-8.2.2
while read -r p; do
  case "$p" in ''|\#*) continue;; esac
  patch -p1 -s -N --no-backup-if-mismatch < "debian/patches/$p"
done < debian/patches/series

# 3. meson 装进工作区 venv（清华 PyPI）
cd .. && python3 -m venv venv && ./venv/bin/pip install -i https://pypi.tuna.tsinghua.edu.cn/simple meson

# 4. 编译安装（务必绕过 ccache，见下方「注意事项」）
cd qemu-8.2.2
export PATH="$PWD/../venv/bin:/usr/bin:/bin" CC=/usr/bin/gcc
./configure --prefix="$PWD/../install" \
  --target-list=riscv64-softmmu,riscv64-linux-user \
  --disable-werror --disable-docs --disable-tools --disable-install-blobs
make -j$(nproc) && make install

# 5. 放回项目
cd .. && mkdir -p ../.qemu && cp -r install ../.qemu/8.2.2
```

编译约 1 分钟（22 核）。

## 注意事项

1. **ccache 必须绕过**。`/usr/lib/ccache` 在 PATH 最前，而
   `/run/user/1000/ccache-tmp` 写入失败时会报
   `Unrecognized host OS (uname -s reports 'Linux')`——这个报错有很强的误导性，
   真实原因是编译器根本没跑起来。设置 `CC=/usr/bin/gcc` 并把 `/usr/bin` 放在
   PATH 前即可。

2. **`--disable-install-blobs` 是必需的**。Ubuntu 的 `+ds` 源码包剔除了非自由
   固件（`bios.bin`、`vgabios.bin` 等 x86 二进制），meson 会因文件缺失报错。
   riscv64 不需要这些 blob。

3. `make env` 每次都会执行 `cargo install cargo-binutils --version 0.4.0 --locked`。
   该工具已安装，但在受限环境中 `~/.cargo` 不可写会失败；
   此时用 `make -C os run OFFLINE=1` 跳过。
