# 呓语 (MirageGPU) — TODO

> 第二个功能模块。「呓语」= 说梦话:一块会"梦"出图像的 GPU。

---

## 0. 定位(一句话)

**Guest 没有物理 GPU,却能发现一块 GPU、加载驱动、提交 GPU 命令,并得到一个符合协议的结果。**
第一版**不冒充 NVIDIA/AMD**(避免掉进百万行驱动 / Vulkan / CUDA / shader ISA / 固件的泥潭),
自研一块极简 PCI **Generative GPU**。

---

## 1. 核心命题

> **Hardware interface ≠ Physical hardware**

这正是 MirageVM 的核心。软件模拟 GPU 本质都可由 CPU 完成(QEMU 软件图形、Mesa LLVMpipe、SwiftShader 都是例证),
所以"画图"本身不构成有趣点。有趣的是:

- 传统设备接口:**低语义输入 → 大量确定性计算 → 输出**(百万顶点 + shader + texture → framebuffer)
- MirageGPU:**高语义输入 → generative computation → 输出**(intent + constraints → framebuffer)

由此引出一个比"假 GPU"更有价值的问题:

> **如果未来生成模型成为计算资源,它应该通过什么样的硬件抽象暴露给操作系统?**

现在的普遍做法是 `Application → HTTP API → AI service`。呓语在问:`Application → syscall/driver → PCI device → Generative processor` 是否也成立?

---

## 2. 项目全景(三组件)

```
MirageVM
├── MirageNIC   ← 楚门:生成不存在的网络世界(已开工,阶段 1 机制已证明)
├── MirageGPU   ← 呓语:生成不存在的计算设备(本文档)
└── MirageWorld ← 保持所有"幻觉"前后一致(跨模块共享的 World State)
```

最强第一版演示:**`curl 一个不存在的网站` + `调用一块不存在的 GPU 生成一张图`**。

---

## 3. 设备定义(PCI MirageGPU)

```
PCI: Mirage Generative GPU
  Vendor ID: 0x????   (自定)
  Device ID: 0x????
  BAR0: command registers (MMIO)
  BAR1: framebuffer     (guest 可 mmap)
  command ring + MMIO doorbell
  DMA 回 Guest RAM + MSI-X IRQ
```

Guest 视角:`/dev/mirage-gpu0`,userspace 用 ioctl 提交命令。

---

## 4. 命令集(ISA,分两期)

**Phase 1 —— 确定性命令(证明 plumbing,CPU 光栅化即可):**

```
MGPU_CLEAR
MGPU_DRAW_RECT
MGPU_DRAW_TRIANGLE
MGPU_RENDER
```

**Phase 2 —— 生成式命令(呓语真正的差异化):**

```
MGPU_GENERATE        # (prompt, w, h) → framebuffer   ← 首发
MGPU_GENERATE_IMAGE
MGPU_GENERATE_TEXTURE
MGPU_GENERATE_SCENE
MGPU_UPSCALE
MGPU_DESCRIBE_FRAME
```

远期:`GENERATE_3D_OBJECT / GENERATE_SHADER / SEMANTIC_SEGMENT / INPAINT_REGION`。

---

## 5. 里程碑

### 呓语 M0 —— 设备可见 + 驱动加载 + 一次 MMIO 往返

- [ ] **选型调研**:确定 PCI 设备载体,三选一并出结论
  - (a) vfio-user 用户态设备后端(Rust 进程即设备,符合"Rust backend"定位,推荐方向)
  - (b) QEMU `edu` 设备改造(现成、带 MMIO+DMA+IRQ,当快速热身)
  - (c) 自写 QEMU C 设备模型(fork edu,最可控但最重)
- [ ] 定 PCI vendor/device ID、BAR0 寄存器布局、BAR1 framebuffer 布局
- [ ] guest 侧:最小 Linux 内核驱动(`pci_driver` probe → 注册 `/dev/mirage-gpu0` → mmap BAR1 → ioctl)
- [ ] guest 侧:userspace 测试程序(open + ioctl CLEAR + mmap 读回)
- [ ] **验收**:guest 里 `lspci` 看到 Mirage Generative GPU;驱动成功 probe;清屏读回全 0

### 呓语 M1 —— 完整 draw_triangle 链路(确定性)

- [ ] command ring 结构定义 + MMIO doorbell 语义
- [ ] Rust backend:收 doorbell → 读 command ring → CPU 光栅化 `DRAW_TRIANGLE` → 写 framebuffer → DMA 回 guest RAM → MSI-X IRQ
- [ ] guest 驱动:提交命令 + 等 IRQ + 读回 framebuffer 校验像素
- [ ] **验收**:guest 提交三角形,读回的 framebuffer 像素颜色符合预期(证明"guest 认为完成了一次 GPU 工作,host 无 GPU")

### 呓语 M2 —— `MGPU_GENERATE`(生成式,呓语本体)

- [ ] 定义 `MGPU_GENERATE` 命令(prompt + width + height)
- [ ] 图像生成后端选型(接图像模型 API / 本地模型 / 复用 DeepSeek 走同一个 provider?)
- [ ] 后端:prompt → image → 写 framebuffer → DMA → IRQ
- [ ] **验收**:guest 调 `mgpu_generate("一只猫坐在月球")` → 屏幕上出现图

---

## 6. 关键技术风险 / 待调研

1. **设备载体是最大风险**。vfio-user 的 Rust 侧生态尚在成熟;QEMU 是否要重编、是否支持 vfio-user,需要在 M0 一开始就探明。
2. **DMA + IRQ 的正确性**:后端写进 guest RAM 的物理地址映射、MSI-X 投递,是驱动能否收到结果的关键,也最容易出错。
3. **图像模型延迟**:M2 一次生成可能秒级,guest 侧要用异步(提交后等 IRQ,别同步阻塞)。
4. **与 MirageWorld 打通**:GPU 生成的语义状态要进共享 World State,保证跨模块自洽(例如 `GENERATE_SCENE` 生成的"场景"被后续命令引用)。

---

## 7. 明确不做(v1)

- ❌ 冒充 NVIDIA/AMD/CUDA/OpenGL/Vulkan 兼容
- ❌ 完整 shader ISA / 真 3D 管线
- ❌ 完整 DRM/KMS 显示栈(第一版用 framebuffer 读回即可,不上真屏幕)
