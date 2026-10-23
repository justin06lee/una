import Foundation

public struct ServerEntry: Codable, Sendable, Equatable, Identifiable, Hashable {
    public var id: UUID
    public var name: String
    public var url: String
    public init(id: UUID = UUID(), name: String, url: String) { self.id = id; self.name = name; self.url = url }

    public static let homeDefaults = [
        ServerEntry(name: "tenet (home LAN)", url: "http://192.168.1.172:8100"),
        ServerEntry(name: "tenet (makima)", url: "http://tenet.makima:8100"),
    ]
    public var baseURL: URL? {
        var t = url.trimmingCharacters(in: .whitespaces)
        if !t.contains("://") { t = "http://" + t }
        while t.hasSuffix("/") { t.removeLast() }
        return URL(string: t)
    }
}

public struct ProbeResult: Sendable, Equatable {
    public let entry: ServerEntry
    public let health: Health?
    public let error: String?
    public let latencyMs: Int
    public var ok: Bool { health != nil }
}

/// Races the address list like the desktop's endpoint.rs: every candidate is probed at once
/// with GET /v1/health, the first success wins and is cached for five minutes; a failed
/// upload (or a network path change) drops the cache.
public actor EndpointRace {
    public typealias Probe = @Sendable (ServerEntry) async -> ProbeResult
    public static let cacheTTL: TimeInterval = 300

    private let probe: Probe
    private let now: @Sendable () -> Date
    private var cached: (entry: ServerEntry, at: Date)?

    public init(probe: @escaping Probe = EndpointRace.healthProbe, now: @escaping @Sendable () -> Date = { Date() }) {
        self.probe = probe; self.now = now
    }

    public static let healthProbe: Probe = { entry in
        let t0 = Date()
        guard let base = entry.baseURL else {
            return ProbeResult(entry: entry, health: nil, error: "bad address", latencyMs: 0)
        }
        do {
            let h = try await UnaClient(baseURL: base).health()
            return ProbeResult(entry: entry, health: h, error: nil, latencyMs: Int(Date().timeIntervalSince(t0) * 1000))
        } catch {
            return ProbeResult(entry: entry, health: nil, error: error.localizedDescription,
                               latencyMs: Int(Date().timeIntervalSince(t0) * 1000))
        }
    }

    public func invalidate() { cached = nil }

    /// The winning server, or nil if none answered.
    public func resolve(_ servers: [ServerEntry]) async -> ServerEntry? {
        if let c = cached, now().timeIntervalSince(c.at) < Self.cacheTTL, servers.contains(c.entry) { return c.entry }
        cached = nil
        let probe = self.probe
        let winner: ServerEntry? = await withTaskGroup(of: ProbeResult.self) { group in
            for s in servers { group.addTask { await probe(s) } }
            for await r in group where r.ok { group.cancelAll(); return r.entry }
            return nil
        }
        if let winner { cached = (winner, now()) }
        return winner
    }

    /// Probe every entry (the "Test all" button), results in list order.
    public func testAll(_ servers: [ServerEntry]) async -> [ProbeResult] {
        let probe = self.probe
        return await withTaskGroup(of: (Int, ProbeResult).self) { group in
            for (i, s) in servers.enumerated() { group.addTask { (i, await probe(s)) } }
            var out = [(Int, ProbeResult)]()
            for await r in group { out.append(r) }
            return out.sorted { $0.0 < $1.0 }.map(\.1)
        }
    }
}
