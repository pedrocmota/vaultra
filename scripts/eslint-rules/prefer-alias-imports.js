const path = require('path')

module.exports = {
  meta: {
    type: 'suggestion',
    fixable: 'code',
    schema: [
      {
        type: 'object',
        properties: {
          alias: {
            type: 'string'
          },
          baseUrl: {
            type: 'string'
          }
        },
        additionalProperties: false
      }
    ],
    messages: {
      preferAlias: 'Use alias "{{ alias }}" instead of relative path "{{ relativePath }}".'
    }
  },
  create(context) {
    const options = context.options[0] || {}
    const alias = options.alias || '@'
    const baseUrl = options.baseUrl || 'src'

    const filename = context.getFilename()
    const fileDir = path.dirname(filename)

    const isRelativePath = (importPath) => {
      return importPath.startsWith('./') || importPath.startsWith('../')
    }

    const resolveToAlias = (importPath) => {
      const absolutePath = path.resolve(fileDir, importPath)
      const srcIndex = absolutePath.indexOf(baseUrl + path.sep)

      if (srcIndex === -1) {
        const srcIndexAlt = absolutePath.indexOf(baseUrl + '/')

        if (srcIndexAlt === -1) {
          return null
        }

        const relativeTosrc = absolutePath.slice(srcIndexAlt + baseUrl.length + 1)

        return `${alias}/${relativeTosrc.replace(/\\/g, '/')}`
      }

      const relativeToSrc = absolutePath.slice(srcIndex + baseUrl.length + 1)

      return `${alias}/${relativeToSrc.replace(/\\/g, '/')}`
    }

    return {
      ImportDeclaration(node) {
        const importPath = node.source.value

        if (!isRelativePath(importPath)) {
          return
        }

        if (!importPath.startsWith('../')) {
          return
        }

        const aliasPath = resolveToAlias(importPath)

        if (!aliasPath) {
          return
        }

        context.report({
          node: node.source,
          messageId: 'preferAlias',
          data: {
            alias: aliasPath,
            relativePath: importPath
          },
          fix(fixer) {
            return fixer.replaceText(node.source, `'${aliasPath}'`)
          }
        })
      }
    }
  }
}