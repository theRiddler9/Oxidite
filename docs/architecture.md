# Oxidite Architecture & Pipeline

Oxidite is an autonomous, cross-platform terminal emulator written entirely in Rust. It directly hosts the shell process over an operating system pseudoterminal (PTY), parses the ANSI/VT control byte stream, tracks terminal state and damage in an in-memory grid, and renders output using GPU-accelerated text and quad pipelines.

---

## 1. Core End-to-End Pipeline

```mermaid
flowchart TD
    subgraph PTY_Subsystem["PTY Subsystem"]
        Shell["Shell Process (pwsh / bash / zsh)"]
        PTY_Slave["PTY Slave"]
        PTY_Master["PTY Master (portable-pty)"]
        Reader_Thread["PTY Reader Thread (blocking read)"]
        Writer["PTY Writer"]
    end

    subgraph Event_Plumbing["Event Loop Dispatch"]
        Proxy["winit EventLoopProxy"]
        AppEvent["AppEvent::PtyOutput(Vec&lt;u8&gt;)"]
    end

    subgraph Terminal_Core["Terminal State Machine"]
        Parser["VT Parser (vte::Parser)"]
        Handler["Handler Trait Dispatch"]
        Terminal["Terminal (Primary & Alt Grids, Modes, Cursor, Title)"]
        DirtyRows["Damage Tracking (dirty_rows)"]
    end

    subgraph Renderer_Engine["GPU Renderer (wgpu + glyphon)"]
        QuadPipeline["Quad Pipeline (Cell BG, Cursor, Selection, Bell)"]
        GlyphPipeline["Glyph Pipeline (GlyphCache, Font Shaping)"]
        Surface["WGPU Surface & Device (DX12 / Vulkan / Metal)"]
    end

    subgraph Windowing["OS Window & Input"]
        Window["Winit Window (OS Surface)"]
        InputHandler["Input Encoder (KeyEvent -> ANSI bytes)"]
    end

    Shell <--> PTY_Slave
    PTY_Slave <--> PTY_Master
    PTY_Master --> Reader_Thread
    Reader_Thread -->|Raw Bytes| Proxy
    Proxy --> AppEvent
    AppEvent --> Parser
    Parser -->|Semantic calls| Handler
    Handler --> Terminal
    Terminal --> DirtyRows

    DirtyRows -->|Redraw on demand| QuadPipeline
    DirtyRows -->|Redraw on demand| GlyphPipeline
    QuadPipeline --> Surface
    GlyphPipeline --> Surface
    Surface --> Window

    Window -->|Keyboard & Mouse Events| InputHandler
    InputHandler -->|VT ANSI Bytes| Writer
    Writer --> PTY_Master
```

---

## 2. Module Ownership & Package Boundaries

```mermaid
graph LR
    subgraph Core_Contracts["Contracts & Shared Traits"]
        H["handler.rs<br/>(Handler Trait)"]
        C["contracts/shared-contracts.md"]
    end

    subgraph Input_Output["I/O & Parsing"]
        PTY["pty.rs<br/>(PtyWriter, PtyReader, ShellCommand)"]
        P["parser.rs<br/>(vte::Perform -> Handler)"]
        IN["input.rs<br/>(KeyEvent -> Bytes, Selection)"]
    end

    subgraph State["Terminal & State"]
        G["grid.rs<br/>(Cell, Cursor, Row, Scrollback)"]
        T["terminal.rs<br/>(Terminal, Modes, Alt Screen)"]
    end

    subgraph Rendering["GPU Graphics"]
        W["renderer/window.rs<br/>(Winit window, size mapping)"]
        GPU["renderer/gpu.rs<br/>(WGPU surface, adapter, device)"]
        F["renderer/font.rs<br/>(cosmic-text CellMetrics)"]
        D["renderer/draw.rs<br/>(Quad & Glyph pipelines)"]
    end

    subgraph Config_Subsystem["Configuration & Themes"]
        CFG["config.rs<br/>(config.toml, notify watcher)"]
        TH["theme.rs<br/>(Theme, Rgb, built-in presets)"]
    end

    subgraph App_Orchestration["Application Loop"]
        APP["app.rs<br/>(Winit ApplicationHandler, AppEvent)"]
        MAIN["main.rs<br/>(CLI flags, entrypoint)"]
    end

    MAIN --> APP
    APP --> PTY
    APP --> P
    P --> H
    H --> T
    T --> G
    APP --> IN
    APP --> W
    APP --> GPU
    APP --> F
    APP --> D
    APP --> CFG
    CFG --> TH
```

---

## 3. Data Flow Rules & Invariants

1. **Non-blocking UI Thread**: The main GUI event loop thread never blocks on PTY I/O. Reading occurs on a dedicated background thread, which communicates asynchronously via `EventLoopProxy<AppEvent>`.
2. **Single-threaded State Mutation**: Only the main UI thread mutates the `Terminal` and `Grid` state.
3. **On-Demand Redrawing**: The renderer redraws strictly upon damaged rows or visual timers (cursor blink, visual bell). When the terminal is idle, CPU usage sits near 0.0%.
4. **Coalesced Batching**: High-volume outputs (`cat`, `seq`) are coalesced in memory so that parsing occurs across combined buffers before issuing GPU render passes.
5. **Strict Hierarchy**:
   - `pty.rs` owns OS process spawning and pipes.
   - `parser.rs` owns escape tokenization with zero knowledge of cells or pixels.
   - `grid.rs` and `terminal.rs` maintain character grids with zero graphics dependencies.
   - `renderer/` inspects terminal state read-only to produce vertex buffers and glyph shapes.
   - `input.rs` inspects terminal modes read-only to produce byte sequences for the PTY.
