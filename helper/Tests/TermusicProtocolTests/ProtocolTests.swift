import Foundation
import Testing
@testable import TermusicProtocol

private func d(_ s: String) -> Data { Data(s.utf8) }
private let real = #"{"state":{"animations":false,"channel":null,"duration":null,"hiss":false,"hiss_level":0,"id":null,"loaded":false,"loop":"off","paused":false,"position":0.0,"shell":"tps-l2","shells":[{"id":"tps-l2","name":"TPS-L2"},{"id":"sunset","name":"SUNSET TAPE"},{"id":"amber","name":"AMBER HI-FI"}],"shuffle":false,"thumbnails":false,"title":null,"volume":0}}"#
private func stateLine(title: String, extra: String = "") -> String {
    #"{"state":{"animations":true,"channel":"Ch","duration":213.0,"hiss":true,"hiss_level":30,"id":"x","loaded":true,"loop":"one","paused":true,"position":72.0,"shell":"amber","shells":[],"shuffle":true,"thumbnails":true,"title":"\#(title)","volume":70\#(extra)}}"#
}
private func state(_ s: String) -> State? { if case .state(let x)? = decodeLine(d(s)) { return x }; return nil }

@Test func decodesRealLine() {
    let s = state(real)
    #expect(s?.shells.map(\.id) == ["tps-l2", "sunset", "amber"])
    #expect(s?.shells[1].name == "SUNSET TAPE")
    #expect(s?.title == nil && s?.duration == nil && s?.hissLevel == 0)
}
@Test func decodesValuesAndUnknownField() {
    let s = state(stateLine(title: "T", extra: #","future":[1]"#))
    #expect(s?.title == "T" && s?.duration == 213 && s?.volume == 70 && s?.loop == "one")
}
@Test func decodesErrorAndRejectsJunk() {
    #expect(decodeLine(d(#"{"error":"bad"}"#)) == .error("bad"))
    #expect(decodeLine(d("garbage")) == nil)
    #expect(decodeLine(Data()) == nil)
}
@Test func requestBytes() {
    #expect(playPause() == d("{\"cmd\":\"play_pause\"}\n"))
    #expect(next() == d("{\"cmd\":\"next\"}\n"))
    #expect(prev() == d("{\"cmd\":\"prev\"}\n"))
    #expect(seek(-5) == d("{\"cmd\":\"seek\",\"secs\":-5}\n"))
    #expect(toggleShuffle() == d("{\"cmd\":\"toggle_shuffle\"}\n"))
    #expect(cycleLoop() == d("{\"cmd\":\"cycle_loop\"}\n"))
    #expect(getState() == d("{\"get\":\"state\"}\n"))
    #expect(set("volume", 40) == d("{\"set\":\"volume\",\"value\":40}\n"))
    #expect(set("loop", "all") == d("{\"set\":\"loop\",\"value\":\"all\"}\n"))
    #expect(set("shuffle", true) == d("{\"set\":\"shuffle\",\"value\":true}\n"))
    #expect(set("shell", "amber") == d("{\"set\":\"shell\",\"value\":\"amber\"}\n"))
    #expect(set("thumbnails", false) == d("{\"set\":\"thumbnails\",\"value\":false}\n"))
    #expect(set("animations", true) == d("{\"set\":\"animations\",\"value\":true}\n"))
    #expect(set("hiss", true) == d("{\"set\":\"hiss\",\"value\":true}\n"))
    #expect(set("hiss_level", 30) == d("{\"set\":\"hiss_level\",\"value\":30}\n"))
}
@Test func socketPaths() {
    #expect(socketPath(env: ["TERMUSIC_CONFIG_DIR": "/x"], home: "/h") == "/x/termusic.sock")
    #expect(socketPath(env: ["TERMUSIC_CONFIG_DIR": ""], home: "/h") == "/h/Library/Application Support/termusic/termusic.sock")
    #expect(socketPath(env: [:], home: "/h") == "/h/Library/Application Support/termusic/termusic.sock")
}
@Test func titles() {
    let a30 = String(repeating: "a", count: 30)
    #expect(menuTitle(state(stateLine(title: "Hi")), connected: true) == "Hi")
    #expect(menuTitle(state(stateLine(title: a30 + "a")), connected: true) == a30 + "…")
    #expect(menuTitle(state(stateLine(title: a30)), connected: true) == a30)
    #expect(menuTitle(state(stateLine(title: "")), connected: true) == "♪")
    #expect(menuTitle(state(real), connected: true) == "♪")
    #expect(menuTitle(nil, connected: true) == "♪")
    #expect(menuTitle(state(stateLine(title: "Hi")), connected: false) == "♪")
}
@Test func nullsDecode() {
    let s = state(real)
    #expect(s?.title == nil && s?.channel == nil && s?.duration == nil && s?.id == nil)
}
@Test func appErrorsDecode() {
    #expect(decodeLine(d(#"{"error":"offline"}"#)) == .error("offline"))
    #expect(decodeLine(d(#"{"error":"disabled"}"#)) == .error("disabled"))
}
@Test func missingFieldIsNil() {
    #expect(decodeLine(d(stateLine(title: "T").replacingOccurrences(of: #""volume":70"#, with: #""x":1"#))) == nil)
}
@Test func nullPositionDecodes() {
    let s = state(stateLine(title: "T").replacingOccurrences(of: #""position":72.0"#, with: #""position":null"#))
    #expect(s != nil && s?.position == nil && s?.volume == 70)
}
