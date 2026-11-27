// CSP-safe bounded recursive-descent Plural-Forms evaluator (zero eval / new Function).

const MAX_TOKENS = 128;
const MAX_AST_DEPTH = 16;

type PluralExpr =
  | { kind: 'num'; value: number }
  | { kind: 'n' }
  | { kind: 'not'; inner: PluralExpr }
  | { kind: 'bin'; op: string; lhs: PluralExpr; rhs: PluralExpr }
  | { kind: 'tern'; cond: PluralExpr; t: PluralExpr; f: PluralExpr };

type Token =
  | { kind: 'num'; value: number }
  | { kind: 'n' }
  | { kind: 'sym'; value: string };

function tokenize(src: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  while (i < src.length) {
    if (tokens.length >= MAX_TOKENS) return [];
    const ch = src[i];
    if (ch === ' ' || ch === '\t' || ch === '\r' || ch === '\n') {
      i += 1;
      continue;
    }
    if (ch === 'n') {
      tokens.push({ kind: 'n' });
      i += 1;
      continue;
    }
    if (ch >= '0' && ch <= '9') {
      let j = i;
      while (j < src.length && src[j] >= '0' && src[j] <= '9') j += 1;
      tokens.push({ kind: 'num', value: Number(src.slice(i, j)) });
      i = j;
      continue;
    }
    const two = src.slice(i, i + 2);
    if (two === '||' || two === '&&' || two === '==' || two === '!=' || two === '<=' || two === '>=') {
      tokens.push({ kind: 'sym', value: two });
      i += 2;
      continue;
    }
    if ('()?:+-!%<>'.includes(ch)) {
      tokens.push({ kind: 'sym', value: ch });
      i += 1;
      continue;
    }
    return [];
  }
  return tokens;
}

class Parser {
  private pos = 0;
  constructor(private readonly tokens: Token[]) {}

  private peek(): Token | undefined {
    return this.tokens[this.pos];
  }
  private next(): Token | undefined {
    return this.tokens[this.pos++];
  }
  private isSym(val: string): boolean {
    const t = this.peek();
    return t?.kind === 'sym' && t.value === val;
  }

  parseExpr(depth = 0): PluralExpr {
    if (depth > MAX_AST_DEPTH) throw new Error('Plural-Forms depth exceeded');
    const cond = this.parseOr(depth);
    if (this.isSym('?')) {
      this.next();
      const t = this.parseExpr(depth + 1);
      if (!this.isSym(':')) throw new Error('Expected :');
      this.next();
      const f = this.parseExpr(depth + 1);
      return { kind: 'tern', cond, t, f };
    }
    return cond;
  }

  private parseOr(depth: number): PluralExpr {
    let lhs = this.parseAnd(depth);
    while (this.isSym('||')) {
      this.next();
      lhs = { kind: 'bin', op: '||', lhs, rhs: this.parseAnd(depth) };
    }
    return lhs;
  }

  private parseAnd(depth: number): PluralExpr {
    let lhs = this.parseEq(depth);
    while (this.isSym('&&')) {
      this.next();
      lhs = { kind: 'bin', op: '&&', lhs, rhs: this.parseEq(depth) };
    }
    return lhs;
  }

  private parseEq(depth: number): PluralExpr {
    let lhs = this.parseRel(depth);
    while (this.isSym('==') || this.isSym('!=')) {
      const op = (this.next() as { kind: 'sym'; value: string }).value;
      lhs = { kind: 'bin', op, lhs, rhs: this.parseRel(depth) };
    }
    return lhs;
  }

  private parseRel(depth: number): PluralExpr {
    let lhs = this.parseAdd(depth);
    while (this.isSym('<') || this.isSym('<=') || this.isSym('>') || this.isSym('>=')) {
      const op = (this.next() as { kind: 'sym'; value: string }).value;
      lhs = { kind: 'bin', op, lhs, rhs: this.parseAdd(depth) };
    }
    return lhs;
  }

  private parseAdd(depth: number): PluralExpr {
    let lhs = this.parseMod(depth);
    while (this.isSym('+') || this.isSym('-')) {
      const op = (this.next() as { kind: 'sym'; value: string }).value;
      lhs = { kind: 'bin', op, lhs, rhs: this.parseMod(depth) };
    }
    return lhs;
  }

  private parseMod(depth: number): PluralExpr {
    let lhs = this.parseUnary(depth);
    while (this.isSym('%')) {
      this.next();
      lhs = { kind: 'bin', op: '%', lhs, rhs: this.parseUnary(depth) };
    }
    return lhs;
  }

  private parseUnary(depth: number): PluralExpr {
    if (depth > MAX_AST_DEPTH) throw new Error('Plural-Forms depth exceeded');
    if (this.isSym('!')) {
      this.next();
      return { kind: 'not', inner: this.parseUnary(depth + 1) };
    }
    return this.parsePrimary(depth);
  }

  private parsePrimary(depth: number): PluralExpr {
    if (depth > MAX_AST_DEPTH) throw new Error('Plural-Forms depth exceeded');
    const tok = this.next();
    if (!tok) throw new Error('Unexpected EOF');
    if (tok.kind === 'num') return { kind: 'num', value: tok.value };
    if (tok.kind === 'n') return { kind: 'n' };
    if (tok.kind === 'sym' && tok.value === '(') {
      const inner = this.parseExpr(depth + 1);
      if (!this.isSym(')')) throw new Error('Expected )');
      this.next();
      return inner;
    }
    throw new Error('Invalid Plural-Forms token');
  }
}

function evalExpr(expr: PluralExpr, n: number): number {
  switch (expr.kind) {
    case 'num':
      return expr.value;
    case 'n':
      return n;
    case 'not':
      return evalExpr(expr.inner, n) === 0 ? 1 : 0;
    case 'tern':
      return evalExpr(expr.cond, n) !== 0 ? evalExpr(expr.t, n) : evalExpr(expr.f, n);
    case 'bin': {
      const l = evalExpr(expr.lhs, n);
      const r = evalExpr(expr.rhs, n);
      switch (expr.op) {
        case '||': return l !== 0 || r !== 0 ? 1 : 0;
        case '&&': return l !== 0 && r !== 0 ? 1 : 0;
        case '==': return l === r ? 1 : 0;
        case '!=': return l !== r ? 1 : 0;
        case '<': return l < r ? 1 : 0;
        case '<=': return l <= r ? 1 : 0;
        case '>': return l > r ? 1 : 0;
        case '>=': return l >= r ? 1 : 0;
        case '+': return (l + r) >>> 0;
        case '-': return (l - r) >>> 0;
        case '%': return r === 0 ? 0 : l % r;
        default: return 0;
      }
    }
  }
}

export type PluralEvaluator = (count: number, availableForms: number) => number;

export function compilePluralForms(header: string): PluralEvaluator {
  let nplurals = 2;
  let pluralExprSrc = 'n != 1';
  for (const part of header.split(';')) {
    const t = part.trim();
    if (t.startsWith('nplurals=')) {
      const parsed = Number.parseInt(t.slice(9).trim(), 10);
      if (parsed >= 1 && parsed <= 6) nplurals = parsed;
    } else if (t.startsWith('plural=')) {
      pluralExprSrc = t.slice(7).trim();
    }
  }
  try {
    const tokens = tokenize(pluralExprSrc);
    if (tokens.length === 0) throw new Error('empty');
    const ast = new Parser(tokens).parseExpr(0);
    return (count: number, availableForms: number): number => {
      if (availableForms <= 1) return 0;
      const n = Math.abs(Math.trunc(count));
      const idx = evalExpr(ast, n);
      const maxIdx = Math.max(0, Math.min(nplurals, availableForms) - 1);
      return Math.max(0, Math.min(idx, maxIdx));
    };
  } catch {
    return (count: number, availableForms: number): number =>
      availableForms <= 1 ? 0 : Math.abs(Math.trunc(count)) !== 1 ? 1 : 0;
  }
}
