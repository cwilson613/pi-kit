// Native window identity and clipboard-preserving Ghostty screen export.
import AppKit
import Foundation
import CoreGraphics
let args = Array(CommandLine.arguments.dropFirst())
if args.first == "windows" {
    let windows = CGWindowListCopyWindowInfo(args.contains("--all") ? [.excludeDesktopElements] : [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    let selected = windows.filter { ["Ghostty", "iTerm2", "kitty", "WezTerm", "Terminal"].contains($0["kCGWindowOwnerName"] as? String ?? "") }
    let data = try JSONSerialization.data(withJSONObject: selected, options: [.sortedKeys])
    print(String(data: data, encoding: .utf8)!)
} else if args.first == "clipboard-command" {
    let board = NSPasteboard.general
    let saved = (board.pasteboardItems ?? []).map { item in
        item.types.compactMap { type in item.data(forType: type).map { (type, $0) } }
    }
    defer {
        board.clearContents()
        let items = saved.map { values in
            let item = NSPasteboardItem()
            for (type, data) in values { item.setData(data, forType: type) }
            return item
        }
        board.writeObjects(items)
    }
    let task = Process()
    task.executableURL = URL(fileURLWithPath: args[1])
    task.arguments = Array(args.dropFirst(2))
    task.standardOutput = FileHandle.nullDevice
    try task.run()
    task.waitUntilExit()
    if task.terminationStatus != 0 { throw NSError(domain: "native-command", code: Int(task.terminationStatus)) }
    print(board.string(forType: .string) ?? "")
}
