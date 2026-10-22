// The Apple apps' accent (AccentColor: clay on the icon's ink) as the system
// components' brand colour, so toggles, selects, buttons and emphasis match
// them. App-drawn surfaces use the app.color resources, which carry the
// Codex chat palette (ChatStyle.swift) with dark-mode values.
import { CustomColors, CustomDarkColors, CustomTheme } from '@kit.ArkUI';

const LIGHT_ACCENT = '#A0644B';
const DARK_ACCENT = '#B5775D';

class AccentColors implements CustomColors {
  brand: ResourceColor;
  fontEmphasize: ResourceColor;
  iconEmphasize: ResourceColor;
  compBackgroundEmphasize: ResourceColor;
  compEmphasizeSecondary: ResourceColor;
  interactiveActive: ResourceColor;

  constructor(accent: string) {
    this.brand = accent;
    this.fontEmphasize = accent;
    this.iconEmphasize = accent;
    this.compBackgroundEmphasize = accent;
    this.compEmphasizeSecondary = `#33${accent.substring(1)}`;
    this.interactiveActive = accent;
  }
}

export class MusterTheme implements CustomTheme {
  colors: CustomColors = new AccentColors(LIGHT_ACCENT);
  darkColors: CustomDarkColors = new AccentColors(DARK_ACCENT);
}
