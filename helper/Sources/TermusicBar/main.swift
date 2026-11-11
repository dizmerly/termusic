import AppKit
import TermusicProtocol

if CommandLine.arguments.contains("--selftest") { runSelfTest() }

final class Bar: NSObject {
    let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    let menu = NSMenu()
    var conn: Connection!
    var state: State?
    var connected = false

    let titleItem = NSMenuItem(), channelItem = NSMenuItem()
    let playItem = NSMenuItem(), prevItem = NSMenuItem(), nextItem = NSMenuItem()
    let shuffleItem = NSMenuItem(title: "Shuffle", action: nil, keyEquivalent: "")
    let loopItem = NSMenuItem(title: "Loop", action: nil, keyEquivalent: "")
    let loopMenu = NSMenu()
    let settingsItem = NSMenuItem(title: "Settings", action: nil, keyEquivalent: "")
    let shellItem = NSMenuItem(title: "Shell", action: nil, keyEquivalent: "")
    let shellMenu = NSMenu()
    let thumbsItem = NSMenuItem(title: "Cover art", action: nil, keyEquivalent: "")
    let animItem = NSMenuItem(title: "Animations", action: nil, keyEquivalent: "")
    let hissItem = NSMenuItem(title: "Tape hiss", action: nil, keyEquivalent: "")
    let volume = NSSlider(value: 0, minValue: 0, maxValue: 100, target: nil, action: nil)
    let hissLevel = NSSlider(value: 0, minValue: 0, maxValue: 100, target: nil, action: nil)
    var shownShells: [ShellInfo] = []
    var lastSent: [ObjectIdentifier: Date] = [:]
    let loopModes = [("Off", "off"), ("This track", "one"), ("Whole queue", "all")]

    func act(_ title: String, _ sel: Selector) -> NSMenuItem {
        let i = NSMenuItem(title: title, action: sel, keyEquivalent: "")
        i.target = self
        return i
    }

    func sliderItem(_ label: String, _ s: NSSlider, _ sel: Selector) -> NSMenuItem {
        s.target = self
        s.action = sel
        s.isContinuous = true
        s.frame = NSRect(x: 16, y: 2, width: 168, height: 20)
        let name = NSTextField(labelWithString: label)
        name.font = NSFont.menuFont(ofSize: 0)
        name.frame = NSRect(x: 16, y: 24, width: 168, height: 16)
        let v = NSView(frame: NSRect(x: 0, y: 0, width: 200, height: 44))
        v.addSubview(name)
        v.addSubview(s)
        let i = NSMenuItem()
        i.view = v
        return i
    }

    func setup() {
        menu.autoenablesItems = false
        for i in [titleItem, channelItem] { i.isEnabled = false }
        playItem.target = self; playItem.action = #selector(playPauseTapped)
        prevItem.title = "Previous"; prevItem.target = self; prevItem.action = #selector(prevTapped)
        nextItem.title = "Next"; nextItem.target = self; nextItem.action = #selector(nextTapped)
        shuffleItem.target = self; shuffleItem.action = #selector(shuffleTapped)
        for (n, v) in loopModes {
            let i = act(n, #selector(loopTapped(_:)))
            i.representedObject = v
            loopMenu.addItem(i)
        }
        loopMenu.autoenablesItems = false
        loopItem.submenu = loopMenu
        shellMenu.autoenablesItems = false
        shellItem.submenu = shellMenu
        let settings = NSMenu()
        settings.autoenablesItems = false
        thumbsItem.target = self; thumbsItem.action = #selector(thumbsTapped)
        animItem.target = self; animItem.action = #selector(animTapped)
        hissItem.target = self; hissItem.action = #selector(hissTapped)
        let hissSlider = sliderItem("Hiss level", hissLevel, #selector(hissLevelMoved))
        for i in [shellItem, thumbsItem, animItem, hissItem, hissSlider] { settings.addItem(i) }
        settingsItem.submenu = settings
        let volumeItem = sliderItem("Volume", volume, #selector(volumeMoved))
        let quit = NSMenuItem(title: "Quit TermusicBar", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        for i in [titleItem, channelItem, .separator(), playItem, prevItem, nextItem, .separator(), volumeItem,
                  .separator(), shuffleItem, loopItem, settingsItem, .separator(), quit] { menu.addItem(i) }
        item.menu = menu
        conn = Connection(
            path: socketPath(env: ProcessInfo.processInfo.environment, home: NSHomeDirectory()),
            onState: { [weak self] s in self?.state = s; self?.connected = true; self?.render() },
            onDisconnect: { [weak self] in self?.connected = false; self?.render() })
        render()
        conn.start()
    }

    func render() {
        let s = connected ? state : nil
        let title = menuTitle(s, connected: connected)
        item.button?.attributedTitle = NSAttributedString(
            string: title,
            attributes: connected ? [:] : [.foregroundColor: NSColor.disabledControlTextColor])
        if let s, s.loaded {
            titleItem.title = s.title ?? ""
            channelItem.title = s.channel ?? ""
            channelItem.isHidden = s.channel == nil
        } else {
            titleItem.title = connected ? "Nothing playing" : "termusic is not running"
            channelItem.isHidden = true
        }
        playItem.title = (s?.loaded == true && s?.paused == false) ? "Pause" : "Play"
        shuffleItem.state = s?.shuffle == true ? .on : .off
        for i in loopMenu.items { i.state = (i.representedObject as? String) == s?.loop ? .on : .off }
        thumbsItem.state = s?.thumbnails == true ? .on : .off
        animItem.state = s?.animations == true ? .on : .off
        hissItem.state = s?.hiss == true ? .on : .off
        // Leave a slider alone while the user drags it.
        if let s, NSApp.currentEvent?.type != .leftMouseDragged {
            volume.integerValue = s.volume
            hissLevel.integerValue = s.hissLevel
        }
        // Rebuild only when the list changes; replacing items under the cursor breaks an open submenu.
        let shells = s?.shells ?? []
        if shells != shownShells {
            shownShells = shells
            shellMenu.removeAllItems()
            for sh in shells {
                let i = act(sh.name, #selector(shellTapped(_:)))
                i.representedObject = sh.id
                shellMenu.addItem(i)
            }
        }
        for i in shellMenu.items { i.state = (i.representedObject as? String) == s?.shell ? .on : .off }
        for i in [playItem, prevItem, nextItem, shuffleItem, loopItem, settingsItem, shellItem, thumbsItem, animItem, hissItem] {
            i.isEnabled = connected
        }
        loopMenu.items.forEach { $0.isEnabled = connected }
        shellMenu.items.forEach { $0.isEnabled = connected }
        volume.isEnabled = connected
        hissLevel.isEnabled = connected
    }

    @objc func playPauseTapped() { conn.send(playPause()) }
    @objc func prevTapped() { conn.send(prev()) }
    @objc func nextTapped() { conn.send(next()) }
    @objc func shuffleTapped() { conn.send(set("shuffle", state?.shuffle != true)) }
    @objc func loopTapped(_ i: NSMenuItem) { conn.send(set("loop", i.representedObject as? String ?? "off")) }
    @objc func shellTapped(_ i: NSMenuItem) { conn.send(set("shell", i.representedObject as? String ?? "")) }
    @objc func thumbsTapped() { conn.send(set("thumbnails", state?.thumbnails != true)) }
    @objc func animTapped() { conn.send(set("animations", state?.animations != true)) }
    @objc func hissTapped() { conn.send(set("hiss", state?.hiss != true)) }
    @objc func volumeMoved(_ s: NSSlider) { throttled("volume", s) }
    @objc func hissLevelMoved(_ s: NSSlider) { throttled("hiss_level", s) }

    /// At most 10 sends a second while dragging; the release always sends the final value.
    func throttled(_ key: String, _ s: NSSlider) {
        let id = ObjectIdentifier(s), now = Date()
        let up = NSApp.currentEvent?.type == .leftMouseUp
        if up || now.timeIntervalSince(lastSent[id] ?? .distantPast) >= 0.1 {
            lastSent[id] = now
            conn.send(set(key, s.integerValue))
        }
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let bar = Bar()
bar.setup()
app.run()
