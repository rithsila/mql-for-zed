/// mql-compile-helper.swift
///
/// Minimal Cocoa wrapper that initialises NSApp (required by Wine's macOS
/// display driver) and then runs MetaEditor64.exe /compile inside that context.
///
/// Usage:
///   mql-compile-helper <wine> <wineprefix> <metaeditor_win_path> <mq5_win_path> <include_win_path> <log_win_path>
///
/// Exits with the same code as the wine process (or 1 on argument error).

import AppKit
import Foundation

// ── argument parsing ────────────────────────────────────────────────────────

guard CommandLine.arguments.count == 7 else {
    fputs("usage: mql-compile-helper <wine> <wineprefix> <metaeditor> <mq5> <include> <log>\n", stderr)
    exit(1)
}

let wineExe        = CommandLine.arguments[1]   // /Applications/.../wine/bin/wine
let winePrefix     = CommandLine.arguments[2]   // /Users/.../net.metaquotes.wine.metatrader5
let metaeditorWin  = CommandLine.arguments[3]   // C:\Program Files\MetaTrader 5\MetaEditor64.exe
let mq5Win         = CommandLine.arguments[4]   // Z:\path\to\file.mq5
let includeWin     = CommandLine.arguments[5]   // C:\Program Files\MetaTrader 5
let logWin         = CommandLine.arguments[6]   // Z:\path\to\file.log

let wineLibBase    = (wineExe as NSString).deletingLastPathComponent   // .../wine/bin -> .../wine/bin
let wineLib        = ((wineLibBase as NSString).deletingLastPathComponent as NSString)
                        .appendingPathComponent("lib")                 // .../wine/lib

// ── NSApp initialisation ────────────────────────────────────────────────────
// Wine's winemac.drv calls NSApp APIs from its own threads; we just need
// NSApp to exist and its event loop to be alive while wine runs.

let app = NSApplication.shared
// .regular: gives Wine's macOS driver a real Cocoa app context with proper
// focus, activation, and window-event delivery.
app.setActivationPolicy(.regular)

// ── spawn wine in a background thread ───────────────────────────────────────

var wineExitCode: Int32 = 1

let task = Process()
task.executableURL = URL(fileURLWithPath: wineExe)
task.arguments = [
    metaeditorWin,
    "/compile:\(mq5Win)",
    "/include:\(includeWin)",
    "/log:\(logWin)",
]

var env = ProcessInfo.processInfo.environment
env["WINEPREFIX"]                   = winePrefix
env["WINEDEBUG"]                    = "-all"
env["DYLD_FALLBACK_LIBRARY_PATH"]   = "\(wineLib)/external:\(wineLib)/wine:\(wineLib)"
task.environment = env

// Activate the app so macOS delivers window events to our wine windows.
app.activate(ignoringOtherApps: true)

DispatchQueue.global(qos: .userInitiated).async {
    do {
        try task.run()
        task.waitUntilExit()
        wineExitCode = task.terminationStatus
    } catch {
        fputs("mql-compile-helper: failed to launch wine: \(error)\n", stderr)
        wineExitCode = 1
    }
    // Stop the event loop so this process exits cleanly.
    DispatchQueue.main.async { app.terminate(nil) }
}

// ── run the Cocoa event loop (keeps NSApp alive for winemac.drv) ─────────────
app.run()

exit(wineExitCode)
