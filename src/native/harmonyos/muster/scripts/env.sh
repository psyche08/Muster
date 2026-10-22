# Sourced by build.sh and run.sh: finds hvigor, ohpm, node and SDK 26 in
# Huawei's Command Line Tools (HARMONYOS_COMMAND_LINE_TOOLS) or, failing
# that, in DevEco Studio (DEVECO_STUDIO, default /Applications/DevEco-Studio.app),
# which bundles the same tools under Contents/tools and Contents/sdk.
if [ -n "${HARMONYOS_COMMAND_LINE_TOOLS:-}" ]; then
  HARMONYOS_TOOLS="$HARMONYOS_COMMAND_LINE_TOOLS"
  DEVECO_NODE_HOME="$HARMONYOS_TOOLS/tool/node"
  DEVECO_SDK_HOME="$HARMONYOS_TOOLS/sdk"
else
  DEVECO_STUDIO="${DEVECO_STUDIO:-/Applications/DevEco-Studio.app}"
  HARMONYOS_TOOLS="$DEVECO_STUDIO/Contents/tools"
  DEVECO_NODE_HOME="$HARMONYOS_TOOLS/node"
  DEVECO_SDK_HOME="$DEVECO_STUDIO/Contents/sdk"
  : "${JAVA_HOME:=$DEVECO_STUDIO/Contents/jbr/Contents/Home}"
  export JAVA_HOME
fi
test -x "$HARMONYOS_TOOLS/hvigor/bin/hvigorw" || {
  echo "No hvigor under $HARMONYOS_TOOLS: set HARMONYOS_COMMAND_LINE_TOOLS or DEVECO_STUDIO" >&2
  exit 1
}
export DEVECO_NODE_HOME DEVECO_SDK_HOME
export OHOS_NDK="$DEVECO_SDK_HOME/default/openharmony/native"
export PATH="$HARMONYOS_TOOLS/hvigor/bin:$HARMONYOS_TOOLS/ohpm/bin:$DEVECO_NODE_HOME/bin:$PATH"
HDC="$DEVECO_SDK_HOME/default/openharmony/toolchains/hdc"
