import Darwin
import Dispatch
import Foundation
import TermusicProtocol

/// Unix-socket client. All socket work runs on one background queue; callbacks go to the main queue.
final class Connection {
    private let path: String
    private let q = DispatchQueue(label: "termusic.socket")
    private var fd: Int32 = -1
    private var buf = Data()
    private let onState: (State) -> Void
    private let onDisconnect: () -> Void

    init(path: String, onState: @escaping (State) -> Void, onDisconnect: @escaping () -> Void) {
        self.path = path
        self.onState = onState
        self.onDisconnect = onDisconnect
    }

    func start() { q.async { self.connect() } }

    func send(_ data: Data) {
        q.async {
            guard self.fd >= 0 else { return }
            let ok = data.withUnsafeBytes { Darwin.write(self.fd, $0.baseAddress, data.count) == data.count }
            if !ok { self.drop() }
        }
    }

    private func connect() {
        let s = socket(AF_UNIX, SOCK_STREAM, 0)
        if s >= 0 {
            var one: Int32 = 1
            setsockopt(s, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout<Int32>.size))
            var addr = sockaddr_un()
            addr.sun_family = sa_family_t(AF_UNIX)
            let bytes = Array(path.utf8)
            let cap = MemoryLayout.size(ofValue: addr.sun_path)
            if bytes.count < cap {
                withUnsafeMutableBytes(of: &addr.sun_path) { $0.copyBytes(from: bytes) }
                let ok = withUnsafePointer(to: &addr) {
                    $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                        Darwin.connect(s, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) == 0
                    }
                }
                if ok {
                    fd = s
                    buf = Data()
                    pump()
                    return
                }
            }
            close(s)
        }
        retry()
    }

    private func retry() { q.asyncAfter(deadline: .now() + 2) { self.connect() } }

    private func drop() {
        if fd >= 0 { close(fd); fd = -1 }
        DispatchQueue.main.async(execute: onDisconnect)
        retry()
    }

    /// Poll briefly, read, split lines; re-queues itself so writes can interleave.
    private func pump() {
        guard fd >= 0 else { return }
        var p = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
        if poll(&p, 1, 100) > 0 {
            var tmp = [UInt8](repeating: 0, count: 8192)
            let n = read(fd, &tmp, tmp.count)
            if n <= 0 { return drop() }
            buf.append(contentsOf: tmp[0..<n])
            while let i = buf.firstIndex(of: 10) {
                let l = buf[buf.startIndex..<i]
                buf.removeSubrange(buf.startIndex...i)
                if case .state(let s)? = decodeLine(Data(l)) {
                    DispatchQueue.main.async { self.onState(s) }
                }
            }
        }
        q.async { self.pump() }
    }
}
