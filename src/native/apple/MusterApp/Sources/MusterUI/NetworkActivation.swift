#if os(iOS)
import Foundation
import Network

/// Thin OS adapter. Rust chooses the endpoint, deadline and retry policy.
/// No bytes are sent and readiness is only an OS hint, never a trust decision.
final class NetworkActivation {
    private let connection: NWConnection
    private let queue = DispatchQueue(label: "dev.linsheng.muster.network-activation")
    private var completion: ((Bool, String) -> Void)?

    static func start(address: String, port: UInt32, timeoutMS: UInt32,
                      completion: @escaping (Bool, String) -> Void) {
        guard let number = UInt16(exactly: port), let endpointPort = NWEndpoint.Port(rawValue: number) else {
            completion(false, "Invalid activation port")
            return
        }
        let activation = NetworkActivation(address: address, port: endpointPort, completion: completion)
        activation.connection.stateUpdateHandler = { state in
            NSLog("muster: native network activation %@", String(describing: state))
            switch state {
            case .ready: activation.finish(true, "")
            case .failed(let error): activation.finish(false, error.localizedDescription)
            default: break
            }
        }
        activation.queue.asyncAfter(deadline: .now() + .milliseconds(Int(timeoutMS))) {
            activation.finish(false, "OS network activation timed out")
        }
        activation.connection.start(queue: activation.queue)
    }

    private init(address: String, port: NWEndpoint.Port, completion: @escaping (Bool, String) -> Void) {
        self.connection = NWConnection(host: NWEndpoint.Host(address), port: port, using: .tcp)
        self.completion = completion
    }

    private func finish(_ ok: Bool, _ error: String) {
        guard let completion else { return }
        self.completion = nil
        connection.stateUpdateHandler = nil
        connection.cancel()
        completion(ok, error)
    }
}
#endif
