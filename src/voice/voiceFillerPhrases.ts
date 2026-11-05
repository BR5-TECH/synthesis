/**
 * The phrases of a filler (`VOM-voice-mode.md` VOM-FR-VQGI): a small set for
 * English and for each language of the language table
 * (`VOP-voice-ai-providers.md` VOP-FR-IRZC), keyed by the English name.
 */
export const FILLER_PHRASES: Readonly<Record<string, readonly string[]>> = {
  english: ["Let me think about this.", "One moment, I will check.", "Let me look into this."],
  spanish: ["Un momento, lo pienso.", "Un momento, lo compruebo.", "Voy a revisarlo."],
  french: ["Laissez-moi réfléchir.", "Un instant, je vérifie.", "Je regarde ça."],
  german: ["Moment, ich denke kurz nach.", "Einen Moment, ich prüfe das.", "Ich schaue mir das an."],
  italian: ["Fammi pensare.", "Un momento, controllo.", "Ci do un'occhiata."],
  portuguese: ["Deixe-me pensar.", "Um momento, vou verificar.", "Vou dar uma olhada."],
  russian: ["Дайте подумать.", "Минутку, я проверю.", "Сейчас посмотрю."],
  ukrainian: ["Дайте подумати.", "Хвилинку, я перевірю.", "Зараз подивлюся."],
  polish: ["Daj mi pomyśleć.", "Chwileczkę, sprawdzę.", "Już na to patrzę."],
  dutch: ["Laat me even nadenken.", "Een moment, ik kijk het na.", "Ik zoek het even uit."],
  turkish: ["Bir düşüneyim.", "Bir saniye, kontrol ediyorum.", "Hemen bakıyorum."],
  arabic: ["دعني أفكر في هذا.", "لحظة، سأتحقق من ذلك.", "سأنظر في الأمر."],
  hindi: ["मुझे इस बारे में सोचने दीजिए।", "एक पल रुकिए।", "ज़रा देखने दीजिए।"],
  chinese: ["让我想一想。", "稍等，我查一下。", "我来看看。"],
  japanese: ["少し考えさせてください。", "少々お待ちください、確認します。", "調べてみます。"],
  korean: ["잠시 생각해 볼게요.", "잠시만요, 확인해 볼게요.", "한번 살펴볼게요."],
};

/**
 * VOM-FR-VQGI: a random phrase in `language`, an English name. A language with
 * no set, and no language, use the English set.
 */
export function fillerPhrase(language: string | null | undefined, random: () => number = Math.random): string {
  const set = FILLER_PHRASES[(language ?? "").trim().toLowerCase()] ?? FILLER_PHRASES.english;
  const index = Math.min(set.length - 1, Math.floor(random() * set.length));
  return set[index];
}
