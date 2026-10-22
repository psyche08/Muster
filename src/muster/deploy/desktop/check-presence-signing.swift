import Foundation
import Security

func fail(_ message: String) -> Never {
    fputs("Presence signing: \(message)\n", stderr)
    exit(1)
}
let args = CommandLine.arguments
 guard args.count == 2 || (args.count == 4 && args[2] == "--export-entitlements") else { fail("usage: check-presence-signing.swift Muster.app [--export-entitlements path]") }
let app = URL(fileURLWithPath: args[1])
var code: SecStaticCode?
guard SecStaticCodeCreateWithPath(app as CFURL, [], &code) == errSecSuccess, let code else { fail("cannot inspect code") }
var info: CFDictionary?
guard SecCodeCopySigningInformation(code, SecCSFlags(rawValue: kSecCSSigningInformation), &info) == errSecSuccess, let info = info as? [String: Any], let ent = info[kSecCodeInfoEntitlementsDict as String] as? [String: Any], let team = info[kSecCodeInfoTeamIdentifier as String] as? String, let identifier = info[kSecCodeInfoIdentifier as String] as? String else { fail("missing signing identity or entitlements") }
let appID = team + "." + identifier
guard identifier == "dev.linsheng.muster", ent["com.apple.application-identifier"] as? String == appID, ent["com.apple.developer.team-identifier"] as? String == team, ent["keychain-access-groups"] as? [String] == [appID], ent["com.apple.security.get-task-allow"] as? Bool != true else { fail("missing/mismatched Presence access group or debug entitlement") }
let process = Process(); let output = Pipe(); let errors = Pipe()
process.executableURL = URL(fileURLWithPath: "/usr/bin/security")
process.arguments = ["cms", "-D", "-i", app.appendingPathComponent("Contents/embedded.provisionprofile").path]
process.standardOutput = output; process.standardError = errors
try process.run(); let data = output.fileHandleForReading.readDataToEndOfFile(); process.waitUntilExit()
guard process.terminationStatus == 0, let profile = try PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any], let allowed = profile["Entitlements"] as? [String: Any], profile["ProvisionsAllDevices"] as? Bool == true, let expiry = profile["ExpirationDate"] as? Date, expiry > Date(), profile["TeamIdentifier"] as? [String] == [team], allowed["com.apple.application-identifier"] as? String == appID, let groups = allowed["keychain-access-groups"] as? [String], groups.contains(appID) || groups.contains(team + ".*") else { fail("missing or invalid Developer ID profile for Presence") }
guard let certificates = info[kSecCodeInfoCertificates as String] as? [SecCertificate], let leaf = certificates.first, let allowedCertificates = profile["DeveloperCertificates"] as? [Data], allowedCertificates.contains(SecCertificateCopyData(leaf) as Data) else { fail("profile does not authorize signing certificate") }
if args.count == 4 {
    try PropertyListSerialization.data(fromPropertyList: ent, format: .xml, options: 0).write(to: URL(fileURLWithPath: args[3]))
}
print("Presence signing verified: \(appID)")
