module.exports = {
  meta: {
    type: 'suggestion',
    fixable: 'code',
    schema: [],
    messages: {
      shouldBeCamelCase: 'Parameter Property name "{{ name }}" must be camelCase. Use "{{ camelName }}" instead.'
    }
  },
  create(context) {
    const sourceCode = context.getSourceCode()

    const toCamelCase = (name) => {
      return name.charAt(0).toLowerCase() + name.slice(1)
    }

    const startsWithUpperCase = (name) => {
      return /^[A-Z]/.test(name)
    }

    return {
      MethodDefinition(node) {
        if (node.kind !== 'constructor') {
          return
        }

        const params = node.value.params

        for (const param of params) {
          if (param.type !== 'TSParameterProperty') {
            continue
          }

          const paramNode = param.parameter
          let identifierNode = paramNode

          if (paramNode.type === 'Identifier') {
            identifierNode = paramNode
          } else if (paramNode.type === 'AssignmentPattern' && paramNode.left.type === 'Identifier') {
            identifierNode = paramNode.left
          } else {
            continue
          }

          const name = identifierNode.name

          if (!startsWithUpperCase(name)) {
            continue
          }

          const camelName = toCamelCase(name)

          context.report({
            node: identifierNode,
            messageId: 'shouldBeCamelCase',
            data: {
              name,
              camelName
            },
            fix(fixer) {
              const fixes = []

              const nameStart = identifierNode.range[0]
              const nameEnd = nameStart + name.length
              fixes.push(fixer.replaceTextRange([nameStart, nameEnd], camelName))

              const classBody = node.parent

              if (classBody && classBody.type === 'ClassBody') {
                for (const member of classBody.body) {
                  if (member.type === 'MethodDefinition' && member.value && member.value.body) {
                    const tokens = sourceCode.getTokens(member.value.body)

                    for (let i = 0; i < tokens.length; i++) {
                      const token = tokens[i]

                      if (token.type === 'Keyword' && token.value === 'this') {
                        const dot = tokens[i + 1]
                        const prop = tokens[i + 2]

                        if (
                          dot &&
                          dot.type === 'Punctuator' &&
                          dot.value === '.' &&
                          prop &&
                          prop.type === 'Identifier' &&
                          prop.value === name
                        ) {
                          fixes.push(fixer.replaceText(prop, camelName))
                        }
                      }
                    }
                  }
                }
              }

              return fixes
            }
          })
        }
      }
    }
  }
}
