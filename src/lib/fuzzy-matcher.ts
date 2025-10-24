/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

// Adapted from VSCode's fuzzy scorer for use in command palette fuzzy matching

//#region Character Codes

const enum CharCode {
  Space = 32,
  SingleQuote = 39,
  DoubleQuote = 34,
  Dash = 45,
  Period = 46,
  Slash = 47,
  Backslash = 92,
  Underline = 95,
  Colon = 58,
  A = 65,
  Z = 90,
}

//#endregion

//#region Types

export interface IMatch {
  start: number;
  end: number;
}

export type FuzzyScore = [number /* score */, number[] /* match positions */];

//#endregion

//#region Fuzzy Scorer

const NO_MATCH = 0;
const NO_SCORE: FuzzyScore = [NO_MATCH, []];

function isUpper(code: number): boolean {
  return CharCode.A <= code && code <= CharCode.Z;
}

export function scoreFuzzy(
  target: string,
  query: string,
  queryLower: string,
  allowNonContiguousMatches: boolean
): FuzzyScore {
  if (!target || !query) {
    return NO_SCORE;
  }

  const targetLength = target.length;
  const queryLength = query.length;

  if (targetLength < queryLength) {
    return NO_SCORE;
  }

  const targetLower = target.toLowerCase();
  return doScoreFuzzy(
    query,
    queryLower,
    queryLength,
    target,
    targetLower,
    targetLength,
    allowNonContiguousMatches
  );
}

function doScoreFuzzy(
  query: string,
  queryLower: string,
  queryLength: number,
  target: string,
  targetLower: string,
  targetLength: number,
  allowNonContiguousMatches: boolean
): FuzzyScore {
  const scores: number[] = [];
  const matches: number[] = [];

  for (let queryIndex = 0; queryIndex < queryLength; queryIndex++) {
    const queryIndexOffset = queryIndex * targetLength;
    const queryIndexPreviousOffset = queryIndexOffset - targetLength;

    const queryIndexGtNull = queryIndex > 0;

    const queryCharAtIndex = query[queryIndex];
    const queryLowerCharAtIndex = queryLower[queryIndex];

    for (let targetIndex = 0; targetIndex < targetLength; targetIndex++) {
      const targetIndexGtNull = targetIndex > 0;

      const currentIndex = queryIndexOffset + targetIndex;
      const leftIndex = currentIndex - 1;
      const diagIndex = queryIndexPreviousOffset + targetIndex - 1;

      const leftScore = targetIndexGtNull ? scores[leftIndex] : 0;
      const diagScore = queryIndexGtNull && targetIndexGtNull ? scores[diagIndex] : 0;

      const matchesSequenceLength = queryIndexGtNull && targetIndexGtNull ? matches[diagIndex] : 0;

      let score: number;
      if (!diagScore && queryIndexGtNull) {
        score = 0;
      } else {
        score = computeCharScore(
          queryCharAtIndex,
          queryLowerCharAtIndex,
          target,
          targetLower,
          targetIndex,
          matchesSequenceLength
        );
      }

      const isValidScore =
        score &&
        diagScore + score >= leftScore;
      if (
        isValidScore &&
        (allowNonContiguousMatches ||
          queryIndexGtNull ||
          targetLower.startsWith(queryLower, targetIndex))
      ) {
        matches[currentIndex] = matchesSequenceLength + 1;
        scores[currentIndex] = diagScore + score;
      } else {
        matches[currentIndex] = NO_MATCH;
        scores[currentIndex] = leftScore;
      }
    }
  }

  // Restore positions
  const positions: number[] = [];
  let queryIndex = queryLength - 1;
  let targetIndex = targetLength - 1;
  while (queryIndex >= 0 && targetIndex >= 0) {
    const currentIndex = queryIndex * targetLength + targetIndex;
    const match = matches[currentIndex];
    if (match === NO_MATCH) {
      targetIndex--;
    } else {
      positions.push(targetIndex);
      queryIndex--;
      targetIndex--;
    }
  }

  return [scores[queryLength * targetLength - 1], positions.reverse()];
}

function computeCharScore(
  queryCharAtIndex: string,
  queryLowerCharAtIndex: string,
  target: string,
  targetLower: string,
  targetIndex: number,
  matchesSequenceLength: number
): number {
  let score = 0;

  if (!considerAsEqual(queryLowerCharAtIndex, targetLower[targetIndex])) {
    return score;
  }

  // Character match bonus
  score += 1;

  // Consecutive match bonus
  if (matchesSequenceLength > 0) {
    score += Math.min(matchesSequenceLength, 3) * 6 + Math.max(0, matchesSequenceLength - 3) * 3;
  }

  // Same case bonus
  if (queryCharAtIndex === target[targetIndex]) {
    score += 1;
  }

  // Start of word bonus
  if (targetIndex === 0) {
    score += 8;
  } else {
    // After separator bonus
    const separatorBonus = scoreSeparatorAtPos(target.charCodeAt(targetIndex - 1));
    if (separatorBonus) {
      score += separatorBonus;
    }
    // Inside word upper case bonus (camel case)
    else if (isUpper(target.charCodeAt(targetIndex)) && matchesSequenceLength === 0) {
      score += 2;
    }
  }

  return score;
}

function considerAsEqual(a: string, b: string): boolean {
  if (a === b) {
    return true;
  }

  // Special case path separators: ignore platform differences
  if (a === '/' || a === '\\') {
    return b === '/' || b === '\\';
  }

  return false;
}

function scoreSeparatorAtPos(charCode: number): number {
  switch (charCode) {
    case CharCode.Slash:
    case CharCode.Backslash:
      return 5;
    case CharCode.Underline:
    case CharCode.Dash:
    case CharCode.Period:
    case CharCode.Space:
    case CharCode.SingleQuote:
    case CharCode.DoubleQuote:
    case CharCode.Colon:
      return 4;
    default:
      return 0;
  }
}

//#endregion

//#region Helper Functions

export function createMatches(offsets: number[] | undefined): IMatch[] {
  const ret: IMatch[] = [];
  if (!offsets) {
    return ret;
  }

  let last: IMatch | undefined;
  for (const pos of offsets) {
    if (last && last.end === pos) {
      last.end += 1;
    } else {
      last = { start: pos, end: pos + 1 };
      ret.push(last);
    }
  }

  return ret;
}

//#endregion

//#region Simple API for Command Palette

export interface FuzzyMatchResult {
  score: number;
  matches: IMatch[];
}

/**
 * Simple fuzzy match function for command palette usage.
 * Returns a score and match positions for highlighting.
 *
 * @param query The search query
 * @param target The string to match against
 * @param allowNonContiguous Whether to allow non-contiguous matches (default: true)
 * @returns Object with score and match positions, or null if no match
 */
export function fuzzyMatch(
  query: string,
  target: string,
  allowNonContiguous = true
): FuzzyMatchResult | null {
  if (!query || !target) {
    return null;
  }

  const queryLower = query.toLowerCase();
  const [score, positions] = scoreFuzzy(target, query, queryLower, allowNonContiguous);

  if (score === 0) {
    return null;
  }

  return {
    score,
    matches: createMatches(positions),
  };
}

//#endregion
