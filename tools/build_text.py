"""Convert an eBible.org USFM translation into the app's text file.

usage: python3 -I tools/build_text.py <dir with USFM files> <out.tsv>

Records, tab separated:
  B code short long     book
  C n                   chapter
  M text                major division (Psalms "BOOK I")
  S text                section heading
  R text                reference line under a heading
  D text                psalm title
  H text                minor heading (Psalm 119 letters, speakers, subscriptions)
  K                     blank line (stanza break)
  V n brk text          first piece of verse n (empty when the text starts on the next line)
  + brk text            further piece of the same verse
  N n text              margin note or cross reference on verse n (0 = before the first verse)
brk says how a piece starts: p paragraph, m paragraph without indent,
1 2 3 poetry or list line at that indent, c centred line, - runs on.
Inline tags: <i> supplied or emphasised words, <n> divine name, <j> words of Jesus.
"""
import os, re, sys

src, out = sys.argv[1], sys.argv[2]
CANON = ('GEN EXO LEV NUM DEU JOS JDG RUT 1SA 2SA 1KI 2KI 1CH 2CH EZR NEH EST JOB PSA PRO ECC SNG ISA JER LAM '
         'EZK DAN HOS JOL AMO OBA JON MIC NAM HAB ZEP HAG ZEC MAL MAT MRK LUK JHN ACT ROM 1CO 2CO GAL EPH PHP '
         'COL 1TH 2TH 1TI 2TI TIT PHM HEB JAS 1PE 2PE 1JN 2JN 3JN JUD REV').split()
BREAKS = {'p': 'p', 'pmo': 'p', 'pi1': 'p', 'm': 'm', 'mi': 'm', 'nb': '-', 'pc': 'c',
          'q1': '1', 'q2': '2', 'q3': '3', 'qr': '2', 'li1': '1', 'li2': '2'}
HEADS = {'ms': 'M', 'ms1': 'M', 's1': 'S', 's2': 'S', 'r': 'R', 'mr': 'R', 'd': 'D', 'qa': 'H', 'sp': 'H'}
SKIP = {'ide', 'usfm', 'toc2', 'toc3', 'mt1', 'mt2', 'mt3', 'cl'}
TAGS = ('add', 'i'), ('it', 'i'), ('qs', 'i'), ('bk', 'i'), ('nd', 'n'), ('wj', 'j')
# Slips in the publisher's BSB files, corrected to match the publisher's own plain-text edition.
TYPOS = ('[’’]', ''), (' vvv ', ' '), ('“evenif ', '“even if ')
NOTE = re.compile(r'\\([fx]) \+ (.*?)\\\1\*')  # footnotes and cross references


def strip_words(t):
    t = re.sub(r'\\\+?w ([^|\\]*)\|[^\\]*\\\+?w\*', r'\1', t)  # Strong's wrappers
    return re.sub(r'\\ref ([^|\\]*)(?:\|[^\\]*)?\\ref\*', r'\1', t)  # linked references


def tidy(t):
    assert '\\' not in t and '|' not in t and '\t' not in t, t
    t = re.sub(r'\s+', ' ', t.replace('¶', ' ')).strip()
    t = re.sub(r'(<[inj]>) ', r' \1', t)      # keep spaces outside the tags
    t = re.sub(r' (</[inj]>)', r'\1 ', t)
    t = re.sub(r'<([inj])>(\s*)</\1>', r'\2', t)
    # a removed note or tag can leave a space stranded before closing punctuation
    t = re.sub(r' +(?=(?:</?[inj]>)*(?:[,.;:?!]|(?<=[,.;:?!] )[”’]))', '', t)
    t = re.sub(r'(?<=[,.;:?!])((?:</?[inj]>)*) +(?=(?:</?[inj]>)*[”’])', r'\1', t)
    t = re.sub(r'([(“‘](?:<[inj]>)*) +(?=\S)', r'\1', t)
    t = re.sub(r' +(?=(?:</[inj]>)*\))', '', t)
    t = re.sub(r'(?<=[A-Za-z])([:,])(?=[A-Za-z])', r'\1 ', t)
    t = re.sub(r'</([inj])>( ?)<\1>', r'\2', t)
    return re.sub(r' +', ' ', t).strip()


def inline(t):
    for typo, fix in TYPOS:
        t = t.replace(typo, fix)
    t = re.sub(r'(?<=[.,:;!?])\\f \+ .*?\\f\*(?=\w)', ' ', t)  # note wedged between two sentences
    t = NOTE.sub('', t)
    t = strip_words(t)
    t = re.sub(r'\\\+?(tl|wh)\*?', '', t)
    for usfm, tag in TAGS:
        t = re.sub(r'\\\+?%s\*' % usfm, '</%s>' % tag, t)
        t = re.sub(r'\\\+?%s(?=\s|$)' % usfm, '<%s>' % tag, t)
    return tidy(t)


def notes(t):
    found = NOTE.findall(t)
    assert len(found) == t.count('\\f*') + t.count('\\x*'), t
    res = []
    for _, n in found:
        n = strip_words(n)
        n = re.sub(r'\\(fr|xo) \S+', '', n)
        n = re.sub(r'\\\+?(ft|fqa|fq|fv|xt|wh|tl|nd|add|it|bk)\*?', '', n)
        res.append(tidy(n))
    return res


files = {}
for root, _, names in os.walk(src):
    for name in names:
        if name.lower().endswith(('.usfm', '.sfm')):
            path = os.path.join(root, name)
            head = open(path, encoding='utf-8-sig').readline().split()
            if head[:1] == ['\\id'] and head[1] in CANON:
                assert head[1] not in files, path
                files[head[1]] = path
assert len(files) == 66, sorted(set(CANON) - set(files))

rows = []
red = False  # a words-of-Jesus span can run across verses
for code in CANON:
    book, brk, verse = {}, 'p', '0'

    def body_of(piece):
        global red
        body = ('<j>' if red else '') + inline(piece)
        red = body.count('<j>') > body.count('</j>')
        return tidy(body + '</j>') if red else body

    def text(piece):
        global brk
        body = body_of(piece)
        if body:
            rows.append(('+', brk, body))
            brk = '-'
        rows.extend(('N', verse, x) for x in notes(piece))

    source = open(files[code], encoding='utf-8-sig').read()
    for line in re.sub(r'\s*\\v ', r'\n\\v ', source).split('\n'):
        line = line.strip()
        if not line:
            continue
        m = re.match(r'\\(\w+)\s*(.*)', line)
        assert m, line
        tag, rest = m.groups()
        if tag == 'id':
            book = {'code': rest.split()[0]}
        elif tag in ('h', 'toc1'):
            book[tag] = rest.strip()
        elif tag in SKIP:
            pass
        elif tag == 'c':
            if rest.strip() == '1':
                rows.append(('B', book['code'], book['h'], book['toc1']))
            rows.append(('C', rest.strip()))
            brk, verse = 'p', '0'
        elif tag == 'b':
            assert not rest, line
            rows.append(('K',))
        elif tag in HEADS:
            rows.append((HEADS[tag], inline(rest)))
            rows.extend(('N', verse, x) for x in notes(rest))
        elif tag in BREAKS:
            brk = BREAKS[tag]
            if rest:
                text(rest)
        elif tag == 'v':
            verse, _, piece = rest.partition(' ')
            assert verse.isdigit(), line
            body = body_of(piece)
            rows.append(('V', verse, brk, body))
            if body:
                brk = '-'
            rows.extend(('N', verse, x) for x in notes(piece))
        else:
            raise SystemExit('unhandled marker: ' + line)

with open(out, 'w', encoding='utf-8') as fh:
    for r in rows:
        assert not any('\t' in x or '\n' in x for x in r), r
        fh.write('\t'.join(r) + '\n')
count = lambda k: sum(r[0] == k for r in rows)
print(count('B'), 'books', count('C'), 'chapters', count('V'), 'verses', count('N'), 'notes')
