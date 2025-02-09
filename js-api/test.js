'use strict';

const { HTMLRewriter } = require('lol-html');

const chunks = [];
const rewriter = new HTMLRewriter('utf8', (chunk) => {
  chunks.push(chunk);
});

rewriter.on('a[href]', {
  element(el) {
    const href = el
      .getAttribute('href')
      .replace('http:', 'https:');
    el.setAttribute('href', href);
  },
});

[
  '<div><a href=',
  'http://example.com>',
  '</a></div>',
].forEach((part) => {
  rewriter.write(Buffer.from(part));
});

rewriter.end();

const output = Buffer.concat(chunks).toString('utf8');
if (output != '<div><a href="https://example.com"></a></div>') {
  throw "fail";
}
