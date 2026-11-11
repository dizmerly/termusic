import Foundation
import TermusicProtocol

/// `--selftest`: the same checks as the test target, for toolchains that cannot run it.
func runSelfTest() -> Never {
    var failed = 0
    func check(_ name: String, _ ok: Bool) {
        print((ok ? "PASS " : "FAIL ") + name)
        if !ok { failed += 1 }
    }
    func d(_ s: String) -> Data { Data(s.utf8) }
    let real = #"{"state":{"animations":false,"channel":null,"duration":null,"hiss":false,"hiss_level":0,"id":null,"loaded":false,"loop":"off","paused":false,"position":0.0,"shell":"tps-l2","shells":[{"id":"tps-l2","name":"TPS-L2"},{"id":"sunset","name":"SUNSET TAPE"},{"id":"amber","name":"AMBER HI-FI"}],"shuffle":false,"thumbnails":false,"title":null,"volume":0}}"#
    var real_state: State?
    if case .state(let s)? = decodeLine(d(real)) { real_state = s }
    check("real state line", real_state?.shells.count == 3 && real_state?.shells[1].name == "SUNSET TAPE" && real_state?.loop == "off" && real_state?.hissLevel == 0)
    let full = #"{"state":{"animations":true,"channel":"Ch","duration":213.0,"hiss":true,"hiss_level":30,"id":"x","loaded":true,"loop":"one","paused":true,"position":72.0,"shell":"amber","shells":[],"shuffle":true,"thumbnails":true,"title":"T","volume":70,"future":[1]}}"#
    if case .state(let s)? = decodeLine(d(full)) {
        check("values and unknown field", s.title == "T" && s.duration == 213 && s.volume == 70 && s.hissLevel == 30 && s.loop == "one")
    } else { check("values and unknown field", false) }
    check("nulls", real_state?.title == nil && real_state?.duration == nil)
    check("error line", decodeLine(d(#"{"error":"unknown set key: foo"}"#)) == .error("unknown set key: foo"))
    check("garbage", decodeLine(d("garbage")) == nil)
    check("empty", decodeLine(Data()) == nil)
    check("req play_pause", playPause() == d("{\"cmd\":\"play_pause\"}\n"))
    check("req next", next() == d("{\"cmd\":\"next\"}\n"))
    check("req prev", prev() == d("{\"cmd\":\"prev\"}\n"))
    check("req seek", seek(-5) == d("{\"cmd\":\"seek\",\"secs\":-5}\n"))
    check("req toggle_shuffle", toggleShuffle() == d("{\"cmd\":\"toggle_shuffle\"}\n"))
    check("req cycle_loop", cycleLoop() == d("{\"cmd\":\"cycle_loop\"}\n"))
    check("req get state", getState() == d("{\"get\":\"state\"}\n"))
    check("set volume", set("volume", 40) == d("{\"set\":\"volume\",\"value\":40}\n"))
    check("set loop", set("loop", "all") == d("{\"set\":\"loop\",\"value\":\"all\"}\n"))
    check("set shuffle", set("shuffle", true) == d("{\"set\":\"shuffle\",\"value\":true}\n"))
    check("set shell", set("shell", "amber") == d("{\"set\":\"shell\",\"value\":\"amber\"}\n"))
    check("set thumbnails", set("thumbnails", false) == d("{\"set\":\"thumbnails\",\"value\":false}\n"))
    check("set animations", set("animations", true) == d("{\"set\":\"animations\",\"value\":true}\n"))
    check("set hiss", set("hiss", true) == d("{\"set\":\"hiss\",\"value\":true}\n"))
    check("set hiss_level", set("hiss_level", 30) == d("{\"set\":\"hiss_level\",\"value\":30}\n"))
    check("socketPath env", socketPath(env: ["TERMUSIC_CONFIG_DIR": "/x"], home: "/h") == "/x/termusic.sock")
    check("socketPath empty env", socketPath(env: ["TERMUSIC_CONFIG_DIR": ""], home: "/h") == "/h/Library/Application Support/termusic/termusic.sock")
    check("socketPath default", socketPath(env: [:], home: "/h") == "/h/Library/Application Support/termusic/termusic.sock")
    func titled(_ title: String) -> State? {
        let j = full.replacingOccurrences(of: #""title":"T""#, with: "\"title\":\"\(title)\"")
        if case .state(let s)? = decodeLine(d(j)) { return s }
        return nil
    }
    let long = String(repeating: "a", count: 31)
    check("title short", menuTitle(titled("Hi"), connected: true) == "♪ Hi")
    check("title 30 kept", menuTitle(titled(String(repeating: "a", count: 30)), connected: true) == "♪ " + String(repeating: "a", count: 30))
    check("title 31 cut", menuTitle(titled(long), connected: true) == "♪ " + String(repeating: "a", count: 30) + "…")
    check("title nil state", menuTitle(nil, connected: true) == "♪")
    check("title no title", menuTitle(real_state, connected: true) == "♪")
    check("title disconnected", menuTitle(titled("Hi"), connected: false) == "♪")
    check("error offline ignored", decodeLine(d(#"{"error":"offline"}"#)) == .error("offline"))
    check("error disabled ignored", decodeLine(d(#"{"error":"disabled"}"#)) == .error("disabled"))
    check("title empty", menuTitle(titled(""), connected: true) == "♪")
    let missing = full.replacingOccurrences(of: #""volume":70,"#, with: "")
    check("missing field is nil", decodeLine(d(missing)) == nil)
    let nullPos = full.replacingOccurrences(of: #""position":72.0"#, with: #""position":null"#)
    if case .state(let s)? = decodeLine(d(nullPos)) { check("null position decodes", s.position == nil && s.volume == 70) }
    else { check("null position decodes", false) }
    print(failed == 0 ? "ALL PASS" : "\(failed) FAILED")
    exit(failed == 0 ? 0 : 1)
}
