// What an answer sounds like when it is read out rather than looked at.
//
// A voice reading Markdown says "star star Price colon star star", reads a
// twelve-line shell script one bracket at a time, and spells out a URL for
// forty seconds. None of that is the answer: it is the notation the answer is
// written in, and on screen the eye skips it without noticing.
//
// So an answer is turned into something a person can listen to before it is
// spoken. Nothing here changes what is on screen. The written answer stays
// exactly as the agent wrote it, and this is only what the voice gets.

// Roughly how much is worth saying out loud in one go.
//
// A page of text takes four minutes to read aloud and nobody in a call wants
// four minutes. The rest is not lost, it is on screen, which is the one thing
// the voice has to say before it stops.
const A_MOUTHFUL = 700;

/**
 * The answer, as something to be heard.
 *
 * @param {string} written what the agent wrote, in Markdown
 * @returns {string} what to say, which may be nothing at all
 */
export function worthSaying(written) {
  if (!written) return "";
  let saying = written;

  // Whole blocks first, while their fences are still there to find them by.
  // A voice reading a shell script aloud is not reading the answer.
  saying = saying.replace(/```[\s\S]*?```/g, " Code, on screen. ");
  // An unclosed fence is a code block that is still being written, and the
  // rest of the answer is inside it.
  saying = saying.replace(/```[\s\S]*$/g, " Code, on screen. ");

  // A table read aloud is a list of pipes. Two rows or more, because a single
  // line with a pipe in it is usually a shell command in a sentence.
  saying = saying.replace(/^(\|.*\|[ \t]*\n){2,}/gm, " A table, on screen. ");

  const said = [];
  for (let line of saying.split("\n")) {
    // A rule is a change of subject and says nothing on its own.
    if (/^\s*([-*_])\s*\1\s*\1[-*_\s]*$/.test(line)) {
      said.push("");
      continue;
    }
    // The markers that say what kind of line this is. A heading is a sentence,
    // a bullet is a sentence, and read out they are all just sentences.
    line = line.replace(/^\s{0,3}#{1,6}\s+/, "");
    line = line.replace(/^\s*>\s?/, "");
    line = line.replace(/^\s*[-*+]\s+/, "");
    line = line.replace(/^\s*\d+[.)]\s+/, "");
    said.push(line);
  }
  saying = said.join("\n");

  // A link is worth hearing by its words. Its address is forty seconds of
  // slashes and the words are already what it was called.
  saying = saying.replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1");
  saying = saying.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1");
  // A bare address has no words, so it is named rather than spelled.
  saying = saying.replace(/\bhttps?:\/\/\S+/g, "a link");

  // Emphasis is a thing the eye sees. Kept as its words, without its marks.
  saying = saying.replace(/\*\*([^*]+)\*\*/g, "$1");
  saying = saying.replace(/__([^_]+)__/g, "$1");
  saying = saying.replace(/(^|\W)\*([^*\n]+)\*(?=\W|$)/g, "$1$2");
  saying = saying.replace(/(^|\W)_([^_\n]+)_(?=\W|$)/g, "$1$2");
  // Inline code is nearly always a filename or a flag, which is worth hearing.
  // Only its backticks go.
  saying = saying.replace(/`([^`]*)`/g, "$1");

  // Whatever is left, as one thing to be said.
  return saying.replace(/\s+/g, " ").trim();
}

/**
 * The first of it, ending on a sentence, and where the rest is.
 *
 * Cut at a full stop rather than at a character, because a voice stopped
 * mid-clause sounds like a voice that has crashed.
 *
 * @param {string} saying what would be said in full
 * @param {number} atMost roughly how many characters is enough
 */
export function enoughOfIt(saying, atMost = A_MOUTHFUL) {
  if (saying.length <= atMost) return saying;

  // The last sentence that ends before the limit. Nothing found means one very
  // long sentence, and a word boundary is the next best place to stop.
  const upTo = saying.slice(0, atMost);
  const ends = Math.max(upTo.lastIndexOf(". "), upTo.lastIndexOf("? "), upTo.lastIndexOf("! "));
  const cut = ends > atMost / 3 ? upTo.slice(0, ends + 1) : upTo.slice(0, upTo.lastIndexOf(" "));
  // Said, not left hanging: a voice that stops early without saying so sounds
  // like one that was cut off, and somebody waits for the rest.
  return `${cut.trim()} The rest of it is on screen.`;
}

/**
 * What to say for an answer, or nothing where there is nothing to say.
 *
 * An answer that was only a code block leaves "Code, on screen", which is
 * worth saying: silence after a question sounds like a failure.
 */
export function toSay(written) {
  return enoughOfIt(worthSaying(written));
}
