# Building
TARGET := riscv64gc-unknown-none-elf
MODE := release
KERNEL_ELF := target/$(TARGET)/$(MODE)/os
KERNEL_BIN := $(KERNEL_ELF).bin
DISASM_TMP := target/$(TARGET)/$(MODE)/asm
OFFLINE :=

# QEMU
# 课程要求 QEMU 8.2.x（评测使用 8.2.2）。项目自带的 8.2.2 优先，避免用到
# ~/.bashrc 里为其他实验准备的旧版 QEMU。
# 需要改用其他版本时: make run QEMU=/path/to/qemu-system-riscv64
QEMU_LOCAL := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/../.qemu/8.2.2/bin/qemu-system-riscv64)
QEMU ?= $(if $(wildcard $(QEMU_LOCAL)),$(QEMU_LOCAL),qemu-system-riscv64)

# BOARD
BOARD := qemu
SBI ?= rustsbi
BOOTLOADER := ../bootloader/$(SBI)-$(BOARD).bin

# Building mode argument
ifeq ($(MODE), release)
	MODE_ARG := --release
endif

# KERNEL ENTRY
KERNEL_ENTRY_PA := 0x80200000

# Binutils
OBJDUMP := rust-objdump --arch-name=riscv64
OBJCOPY := rust-objcopy --binary-architecture=riscv64

CHAPTER ?= $(shell git rev-parse --abbrev-ref HEAD | sed -E 's/ch([0-9])/\1/')
TEST ?= $(CHAPTER)
BASE ?= 1

# Disassembly
DISASM ?= -x

# Run usertests or usershell
TEST ?=

build: env $(KERNEL_BIN)

env:
ifeq ($(OFFLINE),)
	(rustup target list | grep "riscv64gc-unknown-none-elf (installed)") || rustup target add $(TARGET)
	cargo install cargo-binutils --version 0.4.0 --locked
	rustup component add rust-src
	rustup component add llvm-tools-preview
endif

$(KERNEL_BIN): kernel
	@$(OBJCOPY) $(KERNEL_ELF) --strip-all -O binary $@

kernel:
	@make -C ../user build TEST=$(TEST) CHAPTER=$(CHAPTER) BASE=$(BASE)
	@echo Platform: $(BOARD)
	@cargo build $(MODE_ARG)

clean:
	@cargo clean

disasm: kernel
	@$(OBJDUMP) $(DISASM) $(KERNEL_ELF) | less

disasm-vim: kernel
	@$(OBJDUMP) $(DISASM) $(KERNEL_ELF) > $(DISASM_TMP)
	@vim $(DISASM_TMP)
	@rm $(DISASM_TMP)

run: run-inner

run-inner: build
	@$(QEMU) \
		-machine virt \
		-nographic \
		-bios $(BOOTLOADER) \
		-device loader,file=$(KERNEL_BIN),addr=$(KERNEL_ENTRY_PA)

debug: build
	@tmux new-session -d \
		"$(QEMU) -machine virt -nographic -bios $(BOOTLOADER) -device loader,file=$(KERNEL_BIN),addr=$(KERNEL_ENTRY_PA) -s -S" && \
		tmux split-window -h "riscv64-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' -ex 'set arch riscv:rv64' -ex 'target remote localhost:1234'" && \
		tmux -2 attach-session -d

gdbserver: build
	@$(QEMU) -machine virt -nographic -bios $(BOOTLOADER) -device loader,file=$(KERNEL_BIN),addr=$(KERNEL_ENTRY_PA) -s -S

gdbclient:
	@riscv64-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' -ex 'set arch riscv:rv64' -ex 'target remote localhost:1234'

.PHONY: build env kernel clean disasm disasm-vim run-inner gdbserver gdbclient
