import AppKit
import CoreGraphics
import Darwin
import Foundation

guard CommandLine.arguments.count >= 2,
      let gamePid = Int32(CommandLine.arguments[1]), gamePid > 0 else {
    fatalError("Expected the owned game process ID")
}
if CommandLine.arguments.dropFirst(2).contains("--close") {
    guard let application = NSRunningApplication(processIdentifier: gamePid),
          application.terminate() else {
        fatalError("Owned game application rejected its quit request")
    }
    exit(0)
}
let deadline = Date().addingTimeInterval(30)
while Date() < deadline {
    guard kill(gamePid, 0) == 0 else { fatalError("Game exited before opening its window") }
    guard let windows = CGWindowListCopyWindowInfo(
        [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID
    ) as? [[String: Any]] else {
        fatalError("Native GUI QA requires a WindowServer GUI session")
    }
    for window in windows {
        guard (window[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == gamePid,
              (window[kCGWindowLayer as String] as? NSNumber)?.intValue == 0,
              let bounds = window[kCGWindowBounds as String] as? [String: Any],
              let width = bounds["Width"] as? NSNumber,
              let height = bounds["Height"] as? NSNumber,
              width.intValue >= 480, height.intValue >= 320 else { continue }
        let report: [String: Any] = [
            "pid": gamePid, "width": width.intValue, "height": height.intValue, "visible": true
        ]
        print(String(data: try JSONSerialization.data(withJSONObject: report), encoding: .utf8)!)
        exit(0)
    }
    Thread.sleep(forTimeInterval: 0.1)
}
fatalError("No visible game window within 30 seconds")
