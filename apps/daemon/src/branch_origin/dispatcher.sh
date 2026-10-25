#!/bin/sh
# Octoboard's git hook dispatcher, written by octoboardd. The project sessions Octoboard starts
# point core.hooksPath at this directory through GIT_CONFIG_COUNT, and every hook name git can
# look for is a copy of this script. Each copy runs the hook the user's own configuration names,
# exactly where git itself would have found it. A reference transaction that creates branches also
# tells Octoboard which ones the session's git has just created, and one that moves an existing
# branch, like a push of branches, is run past the revert gate once the user's own hook has passed.
set -f
octo_name=${0##*/}
octo_dir=${0%/*}

# Octoboard's own core.hooksPath entry leaves GIT_CONFIG_*, and the user's own entries stay,
# renumbered: the hook run below, and every git it runs in turn, sees the user's configuration
# alone.
case ${GIT_CONFIG_COUNT-} in
'' | *[!0-9]*) ;;
*)
	octo_i=0
	octo_n=0
	while [ "$octo_i" -lt "$GIT_CONFIG_COUNT" ]; do
		eval "octo_k=\${GIT_CONFIG_KEY_$octo_i-} octo_v=\${GIT_CONFIG_VALUE_$octo_i-}"
		octo_i=$((octo_i + 1))
		case $octo_k in
		[Cc][Oo][Rr][Ee].[Hh][Oo][Oo][Kk][Ss][Pp][Aa][Tt][Hh])
			[ "$octo_v" = "$octo_dir" ] && continue
			;;
		esac
		export "GIT_CONFIG_KEY_$octo_n=$octo_k" "GIT_CONFIG_VALUE_$octo_n=$octo_v"
		octo_n=$((octo_n + 1))
	done
	octo_i=$octo_n
	while [ "$octo_i" -lt "$GIT_CONFIG_COUNT" ]; do
		unset "GIT_CONFIG_KEY_$octo_i" "GIT_CONFIG_VALUE_$octo_i"
		octo_i=$((octo_i + 1))
	done
	if [ "$octo_n" -gt 0 ]; then
		export GIT_CONFIG_COUNT="$octo_n"
	else
		unset GIT_CONFIG_COUNT
	fi
	;;
esac

# Whether a piece of configuration could set core.hooksPath, itself or through an include. Keys
# and section names are case-insensitive and a key cannot be split, so anything that sets either
# contains one of these words; matching more than that only costs the slow path.
octo_risky() {
	case $1 in
	*[Hh][Oo][Oo][Kk][Ss][Pp][Aa][Tt][Hh]* | *[Ii][Nn][Cc][Ll][Uu][Dd][Ee]*) return 0 ;;
	esac
	return 1
}

# Whether the configuration file $1 could set core.hooksPath. A missing file sets nothing, as git
# reads it; one that cannot be read is not vouched for.
octo_mentions() {
	[ -e "$1" ] || return 1
	[ -r "$1" ] || return 0
	while IFS= read -r octo_line || [ -n "$octo_line" ]; do
		octo_risky "$octo_line" && return 0
	done <"$1"
	return 1
}

# Sets octo_hook to the hook in the repository's own hooks directory, which is where git looks
# when no configuration names another, and succeeds only when it can tell, without running git,
# that none does. Anything it does not recognise fails it, and the caller asks git instead.
octo_default_hook() {
	# The configuration files below are the ones octoboardd found this git build to read; another
	# build, with system files of its own, is left to git.
	[ "${GIT_EXEC_PATH-}" = @EXEC_PATH@ ] || return 1
	octo_risky "${GIT_CONFIG_PARAMETERS-}" && return 1
	octo_i=0
	while [ "$octo_i" -lt "${GIT_CONFIG_COUNT:-0}" ]; do
		eval "octo_k=\${GIT_CONFIG_KEY_$octo_i-}"
		octo_risky "$octo_k" && return 1
		octo_i=$((octo_i + 1))
	done
	[ -z "${GIT_WORK_TREE+set}" ] || return 1
	# git runs a hook from the top of the worktree, and exports GIT_DIR, absolute, whenever the
	# repository is not the .git found there.
	if [ -n "${GIT_DIR+set}" ]; then
		case $GIT_DIR in
		/*) octo_gd=$GIT_DIR ;;
		*) return 1 ;;
		esac
	elif [ -d .git ]; then
		octo_gd=.git
	elif [ -f .git ]; then
		IFS= read -r octo_line <.git || [ -n "$octo_line" ] || return 1
		case $octo_line in
		'gitdir: '/*) octo_gd=${octo_line#gitdir: } ;;
		'gitdir: '?*) octo_gd=./${octo_line#gitdir: } ;;
		*) return 1 ;;
		esac
	else
		return 1
	fi
	if [ -n "${GIT_COMMON_DIR+set}" ]; then
		case $GIT_COMMON_DIR in
		/*) octo_cd=$GIT_COMMON_DIR ;;
		*) return 1 ;;
		esac
	elif [ -f "$octo_gd/commondir" ]; then
		IFS= read -r octo_line <"$octo_gd/commondir" || [ -n "$octo_line" ] || return 1
		case $octo_line in
		/*) octo_cd=$octo_line ;;
		?*) octo_cd=$octo_gd/$octo_line ;;
		*) return 1 ;;
		esac
	else
		octo_cd=$octo_gd
	fi
	[ -f "$octo_cd/config" ] || return 1
@SYSTEM_SCANS@
	if [ -n "${GIT_CONFIG_SYSTEM-}" ]; then
		octo_mentions "$GIT_CONFIG_SYSTEM" && return 1
	fi
	if [ -n "${GIT_CONFIG_GLOBAL+set}" ]; then
		octo_mentions "$GIT_CONFIG_GLOBAL" && return 1
	else
		octo_mentions "${XDG_CONFIG_HOME:-${HOME-}/.config}/git/config" && return 1
		octo_mentions "${HOME-}/.gitconfig" && return 1
	fi
	octo_mentions "$octo_cd/config" && return 1
	octo_mentions "$octo_gd/config.worktree" && return 1
	octo_hook=$octo_cd/hooks/$octo_name
}

octo_hook=
if ! octo_default_hook; then
	# The path git itself resolves the hook to, core.hooksPath and includes and all. Without it
	# there is no telling whether the user has a hook here, and skipping one silently is the one
	# thing this script must not do.
	octo_hook=$(git rev-parse --git-path "hooks/$octo_name" 2>/dev/null) || {
		echo "octoboard: could not find out where the $octo_name hook is" >&2
		exit 1
	}
fi
case $octo_hook in
"$octo_dir"/*) octo_hook= ;;
/* | ./*) ;;
?*) octo_hook=./$octo_hook ;;
esac
[ -n "$octo_hook" ] && [ -x "$octo_hook" ] || octo_hook=

octo_nl='
'
# Whether this git belongs to a project session, which is what the variables say.
octo_session=
if [ -n "${@SESSION_VARIABLE@-}" ] && [ -n "${@PORT_VARIABLE@-}" ]; then
	octo_session=1
fi
octo_watch=
if [ "$octo_name" = reference-transaction ] && [ -n "$octo_session" ]; then
	case ${1-} in prepared | committed) octo_watch=1 ;; esac
fi

# Reads the hook's input, which git ends every line of with a newline, into octo_input while the
# lines are few and into the file $1 once they are many, so a transaction of thousands of
# references costs a copy rather than a string grown line by line, and sets octo_spool to the file
# when it is used. Every write to the state directory is braced so that one it refuses, as a
# sandbox may, says nothing and leaves the user's hook alone: a failed redirection of `:` itself
# would end the script. Fails, with the input read so far in octo_input and the rest not yet read,
# when the file cannot be written.
octo_read_input() {
	octo_input=
	octo_count=0
	octo_spool=
	while [ "$octo_count" -lt 256 ] && IFS= read -r octo_line; do
		octo_input=$octo_input$octo_line$octo_nl
		octo_count=$((octo_count + 1))
	done
	[ "$octo_count" -lt 256 ] && return 0
	{ { printf '%s' "$octo_input" && cat; } >"$1"; } 2>/dev/null || return 1
	octo_spool=$1
}

# Runs the user's hook, if any, with the arguments of this script over the input octo_read_input
# read, and sets octo_status to its status.
octo_run_hook() {
	octo_status=0
	[ -n "$octo_hook" ] || return 0
	if [ -n "$octo_spool" ]; then
		"$octo_hook" "$@" <"$octo_spool"
	elif [ -n "$octo_input" ]; then
		printf '%s' "$octo_input" | "$octo_hook" "$@"
	else
		"$octo_hook" "$@" </dev/null
	fi
	octo_status=$?
}

# Runs the revert gate over the same input, as the subcommand $1 of Octoboard's gate-hook takes
# it, and makes octo_status 1 when the gate rejects, which it says by exiting 3. Anything else the
# gate comes to, including a binary that cannot start, lets the hook pass. The gate prints its
# reason itself.
octo_run_gate() {
	[ -x @EXE@ ] || return 0
	if [ -n "$octo_spool" ]; then
		@EXE@ gate-hook --session "$@SESSION_VARIABLE@" --port "$@PORT_VARIABLE@" "$@" <"$octo_spool"
	else
		printf '%s' "$octo_input" | @EXE@ gate-hook --session "$@SESSION_VARIABLE@" \
			--port "$@PORT_VARIABLE@" "$@"
	fi
	[ $? -ne 3 ] || octo_status=1
}

# Empties the spool file, if the input was spooled.
octo_clean() {
	[ -z "$octo_spool" ] || { true >"$octo_spool"; } 2>/dev/null
}

# Whether the input has an update to an existing branch: a reference below refs/heads/ with a
# value before and after it, neither all zeros.
octo_moves_branch() {
	if [ -n "$octo_spool" ]; then
		grep -Eq '^[0-9a-f]*[1-9a-f][0-9a-f]* [0-9a-f]*[1-9a-f][0-9a-f]* refs/heads/.' "$octo_spool"
		return
	fi
	IFS=$octo_nl
	for octo_line in $octo_input; do
		octo_old=${octo_line%% *}
		octo_rest=${octo_line#* }
		octo_new=${octo_rest%% *}
		octo_ref=${octo_rest#* }
		case $octo_ref in refs/heads/?*) ;; *) continue ;; esac
		case $octo_old in '' | *[!0-9a-f]*) continue ;; esac
		case $octo_old in *[!0]*) ;; *) continue ;; esac
		case $octo_new in '' | *[!0-9a-f]*) continue ;; esac
		case $octo_new in *[!0]*) unset IFS && return 0 ;; esac
	done
	unset IFS
	return 1
}

# Whether the input, the lines `<local ref> <local sha> <remote ref> <remote sha>` of a push, has
# a local value to push to a reference below refs/heads/, which a deletion has not.
octo_pushes_branch() {
	if [ -n "$octo_spool" ]; then
		grep -Eq '^[^ ]+ [0-9a-f]*[1-9a-f][0-9a-f]* refs/heads/.' "$octo_spool"
		return
	fi
	IFS=$octo_nl
	for octo_line in $octo_input; do
		octo_rest=${octo_line#* }
		octo_new=${octo_rest%% *}
		octo_ref=${octo_rest#* }
		octo_ref=${octo_ref%% *}
		case $octo_ref in refs/heads/?*) ;; *) continue ;; esac
		case $octo_new in '' | *[!0-9a-f]*) continue ;; esac
		case $octo_new in *[!0]*) unset IFS && return 0 ;; esac
	done
	unset IFS
	return 1
}

if [ -n "$octo_watch" ]; then
	if octo_read_input @STATE@/$PPID.in; then
		if [ -n "$octo_spool" ]; then
			octo_lines=$(grep -E '^0+ [0-9a-f]+ refs/heads/.' "$octo_spool")
		else
			octo_lines=$octo_input
		fi
	else
		# A spool that cannot be opened reports nothing and gates nothing, and the user's hook
		# still gets every update.
		[ -n "$octo_hook" ] || exit 0
		{ printf '%s' "$octo_input" && cat; } | "$octo_hook" "$@"
		exit
	fi

	# A branch whose old value is none: created, or forced to a value regardless of what it held,
	# which the transaction cannot tell apart.
	octo_refs=
	octo_names=
	octo_forced=
	IFS=$octo_nl
	for octo_line in $octo_lines; do
		octo_old=${octo_line%% *}
		octo_rest=${octo_line#* }
		octo_new=${octo_rest%% *}
		octo_ref=${octo_rest#* }
		case $octo_ref in refs/heads/?*) ;; *) continue ;; esac
		case $octo_old in '' | *[!0]*) continue ;; esac
		case $octo_new in '' | *[!0-9a-f]*) continue ;; esac
		case $octo_new in *[!0]*) ;; *) continue ;; esac
		octo_refs="$octo_refs $octo_ref $octo_new"
		octo_names="$octo_names $octo_ref"
	done
	unset IFS

	if [ -n "$octo_refs" ]; then
		# Which of them already exist is asked while the transaction is prepared, before any is
		# written, and handed to the same git's committed call: only one that did not exist was
		# created. Nothing later can tell: a forced update to the value a branch already holds
		# writes no reflog entry at all. A reference name holds no whitespace or glob character, so the lists split as
		# written, and each is matched whole.
		octo_known=@STATE@/$PPID
		if [ "$1" = prepared ]; then
			if octo_existing=$(git for-each-ref --format='%(refname)' $octo_names 2>/dev/null); then
				octo_marks=
				for octo_ref in $octo_names; do
					case $octo_nl$octo_existing$octo_nl in
					*"$octo_nl$octo_ref$octo_nl"*)
						octo_marks="${octo_marks}old $octo_ref$octo_nl"
						octo_forced=1
						;;
					*) octo_marks="${octo_marks}new $octo_ref$octo_nl" ;;
					esac
				done
				# A start point of HEAD names no branch of its own: the branch HEAD is on is what
				# a branch created from it was created from. It is read here, before the
				# transaction writes anything, rather than at committed, where a command that
				# also moves HEAD (switch -c, checkout -b) may have done so already.
				case $octo_nl$octo_marks in
				*"${octo_nl}new "*)
					if octo_head=$(git symbolic-ref -q HEAD 2>/dev/null); then
						octo_marks="${octo_marks}head $octo_head$octo_nl"
					fi
					;;
				esac
				{ printf '%s' "$octo_marks" >"$octo_known"; } 2>/dev/null
			else
				{ true >"$octo_known"; } 2>/dev/null
				octo_forced=1
			fi
		else
			octo_marks=
			octo_head=
			if [ -f "$octo_known" ]; then
				while IFS= read -r octo_line; do
					octo_marks=$octo_marks$octo_line$octo_nl
					case $octo_line in 'head '?*) octo_head=${octo_line#head } ;; esac
				done <"$octo_known"
				{ true >"$octo_known"; } 2>/dev/null
			fi
			# Every branch has to be in what prepared wrote, so that a file another process of
			# the same id left behind is never taken for this transaction's.
			octo_created=
			octo_complete=1
			octo_ref=
			for octo_word in $octo_refs; do
				if [ -z "$octo_ref" ]; then
					octo_ref=$octo_word
					continue
				fi
				case $octo_nl$octo_marks in
				*"${octo_nl}new $octo_ref$octo_nl"*) octo_created="$octo_created $octo_ref $octo_word" ;;
				*"${octo_nl}old $octo_ref$octo_nl"*) ;;
				*) octo_complete= ;;
				esac
				octo_ref=
			done
			if [ -n "$octo_complete" ] && [ -n "$octo_created" ]; then
				# In the background with every descriptor let go, so git never waits for it.
				@EXE@ git-ref --session "$@SESSION_VARIABLE@" --port "$@PORT_VARIABLE@" \
					${octo_head:+--head "$octo_head"} -- $octo_created </dev/null >/dev/null 2>&1 &
			fi
		fi
	fi

	# An existing branch forced to a value names no old one (`git branch -f`, `git update-ref`
	# without the old value), so it moves as surely as one that does; so may a branch whose
	# existence could not be asked.
	octo_moving=$octo_forced
	if [ "$1" = prepared ] && [ -z "$octo_moving" ] && octo_moves_branch; then
		octo_moving=1
	fi
	octo_run_hook "$@"
	# The user's hook has the first word: its failure is the transaction's, and the gate is asked
	# only about one that would otherwise go through.
	if [ "$octo_status" -eq 0 ] && [ -n "$octo_moving" ]; then
		octo_run_gate reference-transaction "$1"
	fi
	octo_clean
	exit "$octo_status"
fi

if [ "$octo_name" = pre-push ] && [ -n "$octo_session" ]; then
	# The refs of a push, with the user's hook and the gate run as for a transaction.
	if octo_read_input @STATE@/$PPID.push; then
		octo_run_hook "$@"
		if [ "$octo_status" -eq 0 ] && octo_pushes_branch; then
			octo_run_gate pre-push
		fi
		octo_clean
		exit "$octo_status"
	fi
	[ -n "$octo_hook" ] || exit 0
	{ printf '%s' "$octo_input" && cat; } | "$octo_hook" "$@"
	exit
fi

if [ "$octo_name" = push-to-checkout ] && [ -z "$octo_hook" ]; then
	# A push-to-checkout hook replaces git's own update of the pushed-to worktree, and this
	# script's presence counts as one, so with none of the user's to run it does what git does
	# without a hook, in the worktree git names for it.
	cd "${GIT_WORK_TREE:-..}" || exit 1
	git update-index -q --ignore-submodules --refresh </dev/null >&2 ||
		{ echo "Up-to-date check failed" >&2 && exit 1; }
	git diff-files --quiet --ignore-submodules -- </dev/null >&2 ||
		{ echo "Working directory has unstaged changes" >&2 && exit 1; }
	if git rev-parse -q --verify 'HEAD^{commit}' >/dev/null 2>&1; then
		octo_base=HEAD
	else
		octo_base=$(git hash-object -t tree /dev/null) || exit 1
	fi
	git diff-index --quiet --cached --ignore-submodules "$octo_base" -- </dev/null >&2 ||
		{ echo "Working directory has staged changes" >&2 && exit 1; }
	git read-tree -u -m "$1" </dev/null >&2 ||
		{ echo "Could not update working tree to new HEAD" >&2 && exit 1; }
	exit 0
fi

# The path itself rather than `git hook run`, which would send the hook's stdout to stderr and, on
# a git that also takes hooks from its configuration, run those a second time.
[ -n "$octo_hook" ] || exit 0
exec "$octo_hook" "$@"
