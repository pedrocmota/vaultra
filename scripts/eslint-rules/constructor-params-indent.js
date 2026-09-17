module.exports = {
  meta: {
    type: 'layout',
    fixable: 'whitespace',
    schema: [
      {
        type: 'object',
        properties: {
          indent: {
            type: 'number'
          }
        },
        additionalProperties: false
      }
    ],
    messages: {
      wrongIndent: 'Constructor parameter should be indented with {{ expected }} spaces, but found {{ actual }}.'
    }
  },
  create(context) {
    const options = context.options[0] || {}
    const indentSize = options.indent || 2
    const sourceCode = context.getSourceCode()

    return {
      MethodDefinition(node) {
        if (node.kind !== 'constructor') {
          return
        }

        const params = node.value.params

        if (params.length === 0) {
          return
        }

        const openParen = sourceCode.getTokenBefore(params[0], {filter: t => t.value === '('})
        const closeParen = sourceCode.getTokenAfter(
          params[params.length - 1],
          {filter: t => t.value === ')'}
        )

        if (!openParen || !closeParen) {
          return
        }

        if (openParen.loc.start.line === closeParen.loc.end.line) {
          return
        }

        const constructorIndent = node.loc.start.column
        const expectedIndent = constructorIndent + indentSize

        for (const param of params) {
          const paramLine = param.loc.start.line
          const paramColumn = param.loc.start.column

          if (paramColumn !== expectedIndent) {
            context.report({
              node: param,
              messageId: 'wrongIndent',
              data: {
                expected: expectedIndent,
                actual: paramColumn
              },
              fix(fixer) {
                const lineStart = sourceCode.getIndexFromLoc({line: paramLine, column: 0})
                const paramStart = sourceCode.getIndexFromLoc(
                  {line: paramLine, column: paramColumn}
                )
                
                const correctIndent = ' '.repeat(expectedIndent)

                return fixer.replaceTextRange([lineStart, paramStart], correctIndent)
              }
            })
          }
        }
      }
    }
  }
}