#!/bin/sh
# XcodeGen 2.46.0; project.yml remains the source of truth.
set -eu
cd "$(dirname "$0")/.."
xcodegen generate
mkdir -p Muster.xcodeproj/project.xcworkspace/xcshareddata/swiftpm
cp Package.resolved Muster.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved
