---
description: Guided repository creation with identity awareness and org placement
---
# New Repository

Guide through creating a new repository with proper identity awareness and placement.

## Pre-flight: Identity Check

```bash
echo "Git user: $(git config user.name) <$(git config user.email)>"
gh auth status
gh api user -q '.login'
gh api user/orgs -q '.[].login' 2>/dev/null || echo "(no orgs or limited scope)"
```

Present identity to user. If wrong, fix before proceeding.

## Repo Location: Personal First

Most users should create repos in their personal account first. The recro org requires admin permissions. Workflow:

1. **Create in personal account** (full control)
2. **Work on it** until ready for team collaboration
3. **Request transfer to recro org** — contact Chris Wilson or Will Shepard

## Guided Questions

Ask the user:

1. **Partnership association?** (leidos, qrypt, confluent, pryon, spectro, or none)
2. **Starting point?** From scratch or existing local code?
3. **Visibility?** Private (recommended) or public?

## Execution

### From Scratch

```bash
GH_USER=$(gh api user -q '.login')
gh repo create $1 --private --description "$2"
git clone https://github.com/${GH_USER}/$1.git
```

### From Existing Code

```bash
cd /path/to/existing/code
[[ ! -d .git ]] && git init && git add . && git commit -m "feat: initial commit"
gh repo create $1 --private --source=. --push
```

## Partnership Integration

If associated with a partnership, clone/move into the partnership `repos/` folder and update the partnership's docs with the repo entry.

## Ready for Org Transfer?

Contact Chris Wilson or Will Shepard with:
1. Repo URL
2. Partnership association (if any)
3. Desired org visibility
4. Who needs access
