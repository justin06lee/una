import Foundation

public enum UnaError: Error, Equatable, LocalizedError {
    /// 503: a training run has the GPU. Keep the audio and retry later.
    case trainingInProgress(String)
    case server(status: Int, code: String?, message: String)
    case unreachable(String)
    case badResponse(String)

    public var errorDescription: String? {
        switch self {
        case .trainingInProgress: "Training is using the GPU. Try again shortly."
        case .server(_, _, let m): m
        case .unreachable(let m): "Can't reach the server: \(m)"
        case .badResponse(let m): "Unexpected reply: \(m)"
        }
    }
    /// Worth keeping the audio for a later retry (everything but a bad request).
    public var isRetryable: Bool {
        switch self {
        case .trainingInProgress, .unreachable: true
        case .server(let s, _, _): s >= 500
        case .badResponse: false
        }
    }
}

public struct UnaClient: Sendable {
    public static let connectTimeout: TimeInterval = 3
    public static let requestTimeout: TimeInterval = 60
    public static let healthTimeout: TimeInterval = 2.5

    public let baseURL: URL
    private let session: URLSession
    public let clientName: String

    public init(baseURL: URL, session: URLSession? = nil, clientName: String = "una-ios/0.1 ios-arm64") {
        self.baseURL = baseURL
        self.clientName = clientName
        if let session { self.session = session } else {
            let c = URLSessionConfiguration.ephemeral
            c.timeoutIntervalForRequest = Self.requestTimeout
            c.timeoutIntervalForResource = Self.requestTimeout
            c.waitsForConnectivity = false
            self.session = URLSession(configuration: c)
        }
    }

    private func url(_ path: String, query: [URLQueryItem] = []) -> URL {
        var c = URLComponents(url: baseURL.appendingPathComponent("v1" + path), resolvingAgainstBaseURL: false)!
        if !query.isEmpty { c.queryItems = query }
        return c.url!
    }

    public func health(timeout: TimeInterval = UnaClient.healthTimeout) async throws -> Health {
        var r = URLRequest(url: url("/health")); r.timeoutInterval = timeout
        return try await send(r, as: Health.self)
    }

    /// One utterance. `utteranceId` must be reused on every retry of the same audio.
    /// Retries once, automatically, on a connect/reset error; never on an HTTP status.
    public func createDictation(wav: Data, utteranceId: String, appName: String? = nil,
                                language: String? = nil, clean: Bool = true) async throws -> DictationResponse {
        var form = MultipartForm()
        form.addFile(name: "audio", filename: "audio.wav", mime: "audio/wav", data: wav)
        form.addField("utterance_id", utteranceId)
        form.addField("clean", clean ? "true" : "false")
        form.addField("client", clientName)
        if let appName { form.addField("app_name", appName) }
        if let language { form.addField("language", language) }
        var r = URLRequest(url: url("/dictations")); r.httpMethod = "POST"
        r.setValue(form.contentType, forHTTPHeaderField: "Content-Type")
        r.httpBody = form.body
        do { return try await send(r, as: DictationResponse.self) }
        catch UnaError.unreachable { return try await send(r, as: DictationResponse.self) }
    }

    public func listDictations(limit: Int = 50, cursor: String? = nil) async throws -> DictationList {
        var q = [URLQueryItem(name: "limit", value: String(limit))]
        if let cursor { q.append(URLQueryItem(name: "cursor", value: cursor)) }
        return try await send(URLRequest(url: url("/dictations", query: q)), as: DictationList.self)
    }

    public func dictation(id: String) async throws -> DictationDetail {
        try await send(URLRequest(url: url("/dictations/\(id)")), as: DictationDetail.self)
    }

    public func audio(id: String) async throws -> Data {
        try await sendRaw(URLRequest(url: url("/dictations/\(id)/audio")))
    }

    public func reviewQueue(limit: Int = 20) async throws -> ReviewQueue {
        try await send(URLRequest(url: url("/review/queue", query: [URLQueryItem(name: "limit", value: String(limit))])),
                       as: ReviewQueue.self)
    }

    public func correct(id: String, _ req: CorrectionRequest) async throws -> CorrectionResponse {
        var r = URLRequest(url: url("/dictations/\(id)/correction")); r.httpMethod = "PUT"
        r.setValue("application/json", forHTTPHeaderField: "Content-Type")
        r.httpBody = try UnaCoding.encoder.encode(req)
        return try await send(r, as: CorrectionResponse.self)
    }

    public func delete(id: String) async throws {
        var r = URLRequest(url: url("/dictations/\(id)")); r.httpMethod = "DELETE"
        _ = try await sendRaw(r)
    }

    // MARK: transport

    private func send<T: Decodable>(_ r: URLRequest, as: T.Type) async throws -> T {
        let data = try await sendRaw(r)
        do { return try UnaCoding.decoder.decode(T.self, from: data) }
        catch { throw UnaError.badResponse("\(error)") }
    }

    private func sendRaw(_ r: URLRequest) async throws -> Data {
        let data: Data, resp: URLResponse
        do { (data, resp) = try await session.data(for: r) }
        catch { throw UnaError.unreachable((error as? URLError)?.localizedDescription ?? "\(error)") }
        guard let http = resp as? HTTPURLResponse else { throw UnaError.badResponse("no HTTP response") }
        guard (200..<300).contains(http.statusCode) else {
            let env = try? UnaCoding.decoder.decode(ErrorEnvelope.self, from: data)
            let msg = env?.error.message ?? "HTTP \(http.statusCode)"
            if http.statusCode == 503 { throw UnaError.trainingInProgress(msg) }
            throw UnaError.server(status: http.statusCode, code: env?.error.code, message: msg)
        }
        return data
    }
}

struct MultipartForm {
    let boundary = "una-" + UUID().uuidString
    private var parts = Data()
    var body: Data { parts + Data("--\(boundary)--\r\n".utf8) }
    var contentType: String { "multipart/form-data; boundary=\(boundary)" }

    mutating func addField(_ name: String, _ value: String) {
        parts.append(Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(name)\"\r\n\r\n\(value)\r\n".utf8))
    }
    mutating func addFile(name: String, filename: String, mime: String, data: Data) {
        parts.append(Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(name)\"; filename=\"\(filename)\"\r\nContent-Type: \(mime)\r\n\r\n".utf8))
        parts.append(data); parts.append(Data("\r\n".utf8))
    }
}
