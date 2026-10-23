import Foundation

// Mirrors server/src/una_server/schemas.py. Decoded with snake_case keys.

public struct Timings: Codable, Sendable, Equatable {
    public let transcribeMs: Int
    public let cleanupMs: Int?
    public let totalMs: Int
}

public struct DictationResponse: Codable, Sendable, Equatable {
    public let id: String
    public let text: String
    public let rawText: String
    public let cleanedText: String?
    public let cleanupApplied: Bool
    public let cleanupError: String?
    public let durationMs: Int
    public let timings: Timings
    public let asrModel: String
    public let llmModel: String?
}

public struct DictationSummary: Codable, Sendable, Equatable, Hashable, Identifiable {
    public let id: String
    public let createdAt: String
    public let appName: String?
    public let durationMs: Int
    public let text: String
    public let reviewed: Bool
    public let reviewAction: String?
}

public struct DictationDetail: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let createdAt: String
    public let appName: String?
    public let durationMs: Int
    public let text: String
    public let reviewed: Bool
    public let reviewAction: String?
    public let rawText: String
    public let cleanedText: String?
    public let cleanupApplied: Bool
    public let cleanupDiverged: Bool
    public let asrModel: String?
    public let llmModel: String?
    public let language: String?
    public let correctedText: String?
    public let polishedText: String?
    public let trainingEligible: Bool?
    public let eligibilityReason: String?
    public let correctionSource: String?
    public let evalHoldout: Bool
}

public struct DictationList: Codable, Sendable, Equatable {
    public let items: [DictationSummary]
    public let nextCursor: String?
}

public struct ReviewQueue: Codable, Sendable, Equatable {
    public let items: [DictationDetail]
    public let pending: Int
}

public enum CorrectionAction: String, Codable, Sendable {
    case accepted, edited, skipped, excluded, polished
}

public enum CorrectionSource: String, Codable, Sendable {
    case review, auto, popup
}

public struct CorrectionRequest: Codable, Sendable, Equatable {
    public var correctedText: String?
    public var action: CorrectionAction
    public var polishedText: String?
    public var source: CorrectionSource
    public var transcriptShown: Bool

    public init(action: CorrectionAction, correctedText: String? = nil, polishedText: String? = nil,
                source: CorrectionSource = .review, transcriptShown: Bool = false) {
        self.action = action
        self.correctedText = correctedText
        self.polishedText = polishedText
        self.source = source
        self.transcriptShown = transcriptShown
    }
}

public struct CorrectionResponse: Codable, Sendable, Equatable {
    public let dictationId: String
    public let action: String
    public let normEditDistance: Double?
    public let trainingEligible: Bool
    public let eligibilityReason: String?
    public let polishedText: String?
    public let source: String
}

public struct GPUInfo: Codable, Sendable, Equatable {
    public let name: String?
    public let vramFreeMb: Int?
}

public struct Health: Codable, Sendable, Equatable {
    public let status: String
    public let asrModel: String?
    public let asrModelLoaded: Bool
    public let ollama: String
    public let trainingActive: Bool
    public let gpu: GPUInfo?
}

/// The server's error envelope: {"error": {"code", "message"}}.
public struct ErrorEnvelope: Codable, Sendable, Equatable {
    public struct Body: Codable, Sendable, Equatable { public let code: String; public let message: String }
    public let error: Body
}

public enum UnaCoding {
    public static let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }()
    public static let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.keyEncodingStrategy = .convertToSnakeCase
        return e
    }()
}
