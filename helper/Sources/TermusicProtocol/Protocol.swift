import Foundation

public struct ShellInfo: Decodable, Equatable {
    public let id: String
    public let name: String
}

/// The state line's payload. Unknown extra fields are ignored; decoded by key.
public struct State: Decodable, Equatable {
    public let title: String?
    public let channel: String?
    public let id: String?
    public let loaded: Bool
    public let paused: Bool
    /// Optional so a null position still decodes; the UI treats nil as 0.
    public let position: Double?
    public let duration: Double?
    public let volume: Int
    public let loop: String
    public let shuffle: Bool
    public let shell: String
    public let shells: [ShellInfo]
    public let thumbnails: Bool
    public let animations: Bool
    public let hiss: Bool
    public let hissLevel: Int

    enum CodingKeys: String, CodingKey {
        case title, channel, id, loaded, paused, position, duration, volume, loop, shuffle, shell, shells
        case thumbnails, animations, hiss
        case hissLevel = "hiss_level"
    }
}

public enum Line: Equatable {
    case state(State)
    case error(String)
}

private struct Wire: Decodable {
    let state: State?
    let error: String?
}

/// nil for anything that is not a state or error line.
public func decodeLine(_ data: Data) -> Line? {
    guard let w = try? JSONDecoder().decode(Wire.self, from: data) else { return nil }
    if let s = w.state { return .state(s) }
    if let e = w.error { return .error(e) }
    return nil
}

// MARK: requests (keys sorted, so the bytes are stable)

private func line(_ obj: [String: Any]) -> Data {
    let d = (try? JSONSerialization.data(withJSONObject: obj, options: [.sortedKeys, .withoutEscapingSlashes])) ?? Data()
    return d + Data("\n".utf8)
}

public func playPause() -> Data { line(["cmd": "play_pause"]) }
public func next() -> Data { line(["cmd": "next"]) }
public func prev() -> Data { line(["cmd": "prev"]) }
public func seek(_ secs: Int) -> Data { line(["cmd": "seek", "secs": secs]) }
public func toggleShuffle() -> Data { line(["cmd": "toggle_shuffle"]) }
public func cycleLoop() -> Data { line(["cmd": "cycle_loop"]) }
public func getState() -> Data { line(["get": "state"]) }
/// `key` is one of volume, loop, shuffle, shell, thumbnails, animations, hiss, hiss_level;
/// `value` an Int, Bool or String to match.
public func set(_ key: String, _ value: Any) -> Data { line(["set": key, "value": value]) }

public func socketPath(env: [String: String], home: String) -> String {
    if let d = env["TERMUSIC_CONFIG_DIR"], !d.isEmpty { return d + "/termusic.sock" }
    return home + "/Library/Application Support/termusic/termusic.sock"
}

public func menuTitle(_ s: State?, connected: Bool) -> String {
    guard connected, let t = s?.title, !t.isEmpty else { return "♪" }
    return (t.count > 30 ? String(t.prefix(30)) + "…" : t)
}
