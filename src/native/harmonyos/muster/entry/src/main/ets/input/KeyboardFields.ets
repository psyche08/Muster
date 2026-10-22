// Text fields that type with the in-app keyboard (or the system IME when
// Settings asks for it). The owner keeps the text: edits come back through
// onChange, Enter through onSubmit.
import { FieldEditor, KeyboardTarget, MusterKeyboard, focusField, keyboardPrefs } from './Keyboard';

let nextField = 0;

@ComponentV2
export struct MusterTextArea {
  @Param text: string = '';
  @Param placeholder: string = '';
  /** Enter inserts a newline unless the field submits on Enter. */
  @Param submitOnEnter: boolean = false;
  @Param enterLabel: string = '换行';
  @Param fontSize: number = 16;
  @Param monospace: boolean = false;
  @Param minHeight: number = 36;
  @Param maxHeight: number = 180;
  @Param transparent: boolean = false;
  @Param editable: boolean = true;
  /** Take focus (and so show the keyboard) on appearing and whenever this turns true. */
  @Param autoFocus: boolean = false;
  /** A shorter keyboard that fits below a half-fold crease. */
  @Param compactKeyboard: boolean = false;
  @Event onChange: (text: string) => void = (): void => {};
  @Event onSubmit: () => void = (): void => {};
  private controller: TextAreaController = new TextAreaController();
  private editor: FieldEditor | null = null;
  private readonly fieldId: string = `muster-text-area-${nextField++}`;

  @Monitor('autoFocus')
  focusChanged(): void { if (this.autoFocus) this.focus(); }

  private focus(): void { focusField(this.getUIContext(), this.fieldId); }

  aboutToAppear(): void {
    if (this.autoFocus) this.focus();
    this.editor = new FieldEditor((): string => this.text, (text: string): void => { this.onChange(text); },
      (start: number, end: number): void => { this.show(start, end); },
      (): void => { if (this.submitOnEnter) this.onSubmit(); else this.editor?.target().insert('\n'); },
      (): void => { this.controller.stopEditing(); }, true);
  }

  /** Puts the caret, or a selection the keyboard made, into the field. */
  private show(start: number, end: number): void {
    if (start === end) this.controller.caretPosition(start); else this.controller.setTextSelection(start, end);
  }

  private target(): KeyboardTarget {
    return (this.editor as FieldEditor).target();
  }

  build() {
    if (keyboardPrefs.systemKeyboard) {
      TextArea({ placeholder: this.placeholder, text: this.text, controller: this.controller })
        .fontSize(this.fontSize).fontFamily(this.monospace ? 'monospace' : 'HarmonyOS Sans')
        .constraintSize({ minHeight: this.minHeight, maxHeight: this.maxHeight }).width('100%')
        .backgroundColor(this.transparent ? Color.Transparent : undefined).enabled(this.editable)
        .id(this.fieldId)
        .onChange((value: string): void => { this.onChange(value); })
    } else {
      TextArea({ placeholder: this.placeholder, text: this.text, controller: this.controller })
        .fontSize(this.fontSize).fontFamily(this.monospace ? 'monospace' : 'HarmonyOS Sans')
        .constraintSize({ minHeight: this.minHeight, maxHeight: this.maxHeight }).width('100%')
        .backgroundColor(this.transparent ? Color.Transparent : undefined).enabled(this.editable)
        .customKeyboard(this.keyboard(), { supportAvoidance: true })
        .id(this.fieldId)
        .onChange((value: string): void => { this.onChange(value); })
        .onTextSelectionChange((start: number, end: number): void => { this.editor?.select(start, end); })
    }
  }

  @Builder
  keyboard() {
    MusterKeyboard({ target: this.target(), enterLabel: this.submitOnEnter ? this.enterLabel : '换行',
      compact: this.compactKeyboard })
  }
}

@ComponentV2
export struct MusterTextInput {
  @Param text: string = '';
  @Param placeholder: string = '';
  @Param enterLabel: string = '完成';
  @Param monospace: boolean = false;
  @Param fontSize: number = 16;
  @Param editable: boolean = true;
  @Event onChange: (text: string) => void = (): void => {};
  /** Enter: submits when the owner handles it, otherwise closes the keyboard. */
  @Event onSubmit: () => void = (): void => {};
  private controller: TextInputController = new TextInputController();
  private editor: FieldEditor | null = null;

  aboutToAppear(): void {
    this.editor = new FieldEditor((): string => this.text, (text: string): void => { this.onChange(text); },
      (start: number, end: number): void => { this.show(start, end); },
      (): void => { this.onSubmit(); this.controller.stopEditing(); },
      (): void => { this.controller.stopEditing(); }, false);
  }

  /** Puts the caret, or a selection the keyboard made, into the field. */
  private show(start: number, end: number): void {
    if (start === end) this.controller.caretPosition(start); else this.controller.setTextSelection(start, end);
  }

  private target(): KeyboardTarget {
    return (this.editor as FieldEditor).target();
  }

  build() {
    if (keyboardPrefs.systemKeyboard) {
      TextInput({ placeholder: this.placeholder, text: this.text, controller: this.controller })
        .fontSize(this.fontSize).fontFamily(this.monospace ? 'monospace' : 'HarmonyOS Sans').width('100%')
        .enabled(this.editable)
        .onChange((value: string): void => { this.onChange(value); })
        .onSubmit((): void => { this.onSubmit(); })
    } else {
      TextInput({ placeholder: this.placeholder, text: this.text, controller: this.controller })
        .fontSize(this.fontSize).fontFamily(this.monospace ? 'monospace' : 'HarmonyOS Sans').width('100%')
        .enabled(this.editable)
        .customKeyboard(this.keyboard(), { supportAvoidance: true })
        .onChange((value: string): void => { this.onChange(value); })
        .onTextSelectionChange((start: number, end: number): void => { this.editor?.select(start, end); })
    }
  }

  @Builder
  keyboard() {
    MusterKeyboard({ target: this.target(), enterLabel: this.enterLabel })
  }
}
