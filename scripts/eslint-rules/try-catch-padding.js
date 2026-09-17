module.exports = {
  meta: {
    type: 'layout',
    fixable: 'whitespace',
    messages: {
      needBlankBefore: 'Expected a blank line before try statement.',
      needBlankAfter: 'Expected a blank line after try/catch/finally block.'
    }
  },
  create(context) {
    const sourceCode = context.getSourceCode()

    const hasBlankLineBetween = (a, b) => {
      const lastToken = sourceCode.getLastToken(a)
      const firstToken = sourceCode.getFirstToken(b)

      if (!lastToken || !firstToken) {
        return true
      }

      const textBetween = sourceCode.text.slice(lastToken.range[1], firstToken.range[0])

      return /\n\s*\n/.test(textBetween)
    }

    const getParentBody = (node) => {
      const parent = node.parent

      if (parent.type === 'BlockStatement') {
        return parent.body
      }

      if (parent.type === 'Program') {
        return parent.body
      }

      return null
    }

    const getNodeIndex = (body, node) => {
      return body.findIndex((n) => n === node)
    }

    return {
      TryStatement(node) {
        const body = getParentBody(node)

        if (!body) {
          return
        }

        const index = getNodeIndex(body, node)

        if (index > 0) {
          const prev = body[index - 1]

          if (!hasBlankLineBetween(prev, node)) {
            context.report({
              node,
              messageId: 'needBlankBefore',
              fix(fixer) {
                const firstToken = sourceCode.getFirstToken(node)

                return fixer.insertTextBefore(firstToken, '\n')
              }
            })
          }
        }

        if (index < body.length - 1) {
          const next = body[index + 1]

          if (!hasBlankLineBetween(node, next)) {
            context.report({
              node: next,
              messageId: 'needBlankAfter',
              fix(fixer) {
                const lastToken = sourceCode.getLastToken(node)

                return fixer.insertTextAfter(lastToken, '\n')
              }
            })
          }
        }
      }
    }
  }
}