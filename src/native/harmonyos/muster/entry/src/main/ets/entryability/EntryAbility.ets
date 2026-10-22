// The app's ability: starts the core once, forwards foreground/background,
// hands the window to the model for the fold display.
import { AbilityConstant, UIAbility, Want } from '@kit.AbilityKit';
import { AppStorageV2, ThemeControl, window } from '@kit.ArkUI';
import { MODEL_KEY, MusterModel } from '../core/MusterModel';
import { MusterTheme } from '../theme/MusterTheme';

export default class EntryAbility extends UIAbility {
  private windowGeneration: number = 0;
  readonly model: MusterModel = new MusterModel();

  onCreate(want: Want, launchParam: AbilityConstant.LaunchParam): void {
    AppStorageV2.connect<MusterModel>(MusterModel, MODEL_KEY, (): MusterModel => this.model);
    this.model.start(this.context).catch((): void => {});
  }

  onDestroy(): void {
    this.windowGeneration++;
    this.model.stop();
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    const generation = ++this.windowGeneration;
    ThemeControl.setDefaultTheme(new MusterTheme());
    windowStage.loadContent('pages/Index', (): void => {
      if (generation !== this.windowGeneration) return;
      windowStage.getMainWindow().then((w: window.Window): void => {
        if (generation === this.windowGeneration) this.model.setWindow(w);
      }).catch((): void => {});
    });
  }

  onWindowStageDestroy(): void { this.windowGeneration++; this.model.clearWindow(); }

  onForeground(): void { this.model.foreground(); }
  onBackground(): void { this.model.background(); }
}
