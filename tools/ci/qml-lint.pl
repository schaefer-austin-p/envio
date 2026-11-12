#!/usr/bin/perl
# Two QML rules (see no-qml-plurals.sh): no counted translation calls and no
# swipe rows. Reads the .qml files named on the command line, ignores
# comments and string contents (except a %n inside a translation call), and
# prints "file:line: reason" for each violation. Exit 1 if there is any.
use strict;
use warnings;

my $bad = 0;
# Translation functions and the most arguments each may have without a count.
my %max_args = (qsTr => 2, qsTranslate => 3, qsTrId => 1);

for my $file (@ARGV) {
    open(my $fh, '<', $file) or die "$file: $!";
    local $/;
    my $src = <$fh>;
    close $fh;
    my $n = length $src;
    my $line = 1;
    my $i = 0;
    my $code = '';    # the source with comments and string contents blanked
    my @strings;      # [start offset in $code, text, line] of each string

    # Pass 1: blank comments and strings, keeping offsets and newlines.
    while ($i < $n) {
        my $c = substr($src, $i, 1);
        my $two = substr($src, $i, 2);
        if ($two eq '//') {
            while ($i < $n && substr($src, $i, 1) ne "\n") { $code .= ' '; $i++ }
        } elsif ($two eq '/*') {
            while ($i < $n && substr($src, $i, 2) ne '*/') {
                my $d = substr($src, $i, 1);
                $code .= $d eq "\n" ? "\n" : ' ';
                $i++;
            }
            $code .= '  ';
            $i += 2;
        } elsif ($c eq '"' || $c eq "'" || $c eq '`') {
            my $q = $c;
            my $start = $i;
            $code .= $q;
            $i++;
            while ($i < $n && substr($src, $i, 1) ne $q) {
                if (substr($src, $i, 1) eq '\\') { $code .= ' '; $i++ }
                my $d = substr($src, $i, 1);
                $code .= $d eq "\n" ? "\n" : ' ';
                $i++;
            }
            $code .= $q;
            $i++;
            my $text = substr($src, $start, $i - $start);
            # A template string's ${...} is code this blanking would hide.
            if ($q eq '`' && $text =~ /\$\{/) {
                print "$file:" . line_of($src, $start) . ": template string with \${} (blanked, so not checked; use + or .arg())\n";
                $bad = 1;
            }
            push @strings, [$start, $text];
        } else {
            $code .= $c;
            $i++;
        }
    }

    sub line_of { my ($text, $off) = @_; return 1 + (() = substr($text, 0, $off) =~ /\n/g) }

    # Pass 2: each translation call, its argument count and its strings.
    while ($code =~ /\b(qsTr|qsTranslate|qsTrId)\s*\(/g) {
        my ($fn, $open) = ($1, pos($code));
        my ($depth, $args, $j) = (1, 1, $open);
        my $len = length $code;
        my $empty = 1;
        while ($j < $len && $depth > 0) {
            my $d = substr($code, $j, 1);
            if ($d =~ /[(\[{]/) { $depth++; $empty = 0 }
            elsif ($d =~ /[)\]}]/) { $depth-- }
            elsif ($d eq ',' && $depth == 1) { $args++ }
            elsif ($d !~ /\s/) { $empty = 0 }
            $j++;
        }
        $args = 0 if $empty;
        my $ln = line_of($code, $open);
        if ($args > $max_args{$fn}) {
            print "$file:$ln: $fn with a count argument\n";
            $bad = 1;
        }
        for my $s (@strings) {
            next unless $s->[0] >= $open && $s->[0] < $j;
            if ($s->[1] =~ /%n/) {
                print "$file:" . line_of($src, $s->[0]) . ": %n in $fn\n";
                $bad = 1;
            }
        }
    }

    # Text formats: a Text without textFormat is AutoText, and AutoText
    # turns text that looks like markup (a subject, a sender's name) into
    # styled text, which fetches <img src>. Every Text states its format;
    # AutoText is never one, StyledText only with a "styled-ok" comment on
    # its line or the line above.
    # Label (Controls 2) and a namespaced X.Text are Texts too, and so are
    # TextArea and TextEdit (a body typed or filled from a mailto: link).
    while ($code =~ /(?<![\w.])(?:[A-Za-z_]\w*\.)*(Text|Label|TextArea|TextEdit)\s*\{/g) {
        my ($type, $open) = ($1, pos($code));
        my ($depth, $j, $len, $found) = (1, $open, length $code, 0);
        while ($j < $len && $depth > 0) {
            my $d = substr($code, $j, 1);
            if ($d eq '{') { $depth++ }
            elsif ($d eq '}') { $depth-- }
            elsif ($depth == 1 && substr($code, $j) =~ /^textFormat\s*:/ && substr($code, $j - 1, 1) !~ /[\w.]/) { $found = 1 }
            $j++;
        }
        next if $found;
        my $plain = $type =~ /^Text(Area|Edit)$/ ? 'TextEdit.PlainText' : 'Text.PlainText';
        print "$file:" . line_of($code, $open) . ": $type without textFormat (use $plain)\n";
        $bad = 1;
    }
    my @lines = split /\n/, $src, -1;
    # Any enum scope counts: Text., TextEdit. and Qt. all name the same values.
    while ($code =~ /\b(?:Qt|Text|TextEdit)\.(AutoText|StyledText|MarkdownText)\b/g) {
        my ($kind, $ln) = ($1, line_of($code, $-[0]));
        next if $kind eq 'StyledText'
            && (($lines[$ln - 1] =~ /styled-ok/) || ($ln > 1 && $lines[$ln - 2] =~ /styled-ok/));
        print "$file:$ln: Text.$kind (use Text.PlainText, or Text.RichText for a sanitised body)\n";
        $bad = 1;
    }

    # A count role (unread, count) joined into text or given to .arg() is a
    # count QML formats (#3): the number comes as text from Rust (unreadText,
    # EnvioCore.countText). Arithmetic and comparisons are fine; only
    # concatenation and .arg() are flagged, so a count passed through an
    # alias or a helper function is not caught.
    while ($code =~ /(?:\+(?!=)\s*[\w.\[\]]*\.(?:unread|count)\b(?![\w(])|[\w.\]]\.(?:unread|count)\b\s*\+(?![+=])|\.arg\(\s*[\w.\[\]]*\b(?:unread|count)\b(?![\w(]))/g) {
        print "$file:" . line_of($code, $-[0]) . ": a count role in text (use the text role from Rust, e.g. unreadText)\n";
        $bad = 1;
    }

    # Every textFormat value is a literal enum from a short list, so an alias
    # or an expression cannot carry AutoText past the checks above. (A
    # format set from JS, e.g. `item.textFormat = x`, is not caught.)
    while ($code =~ /(?<![\w.])textFormat\s*:\s*([^\n;}]*)/g) {
        my $v = $1;
        $v =~ s/\s+$//;
        next if $v =~ /^(?:Text|TextEdit|Qt)\.(?:PlainText|RichText|StyledText|AutoText|MarkdownText)$/;
        print "$file:" . line_of($code, $-[0]) . ": textFormat must be a literal (Text.PlainText or Text.RichText), not '$v'\n";
        $bad = 1;
    }

    # fontMono is read only by qml/platform/Theme.qml, which exposes it as
    # fontAddress; labels use fontLabel, reading text fontBody,
    # section labels the eyebrow fields (#67).
    if ($file !~ m{(?:^|/)qml/platform/Theme\.qml$}) {
        while ($code =~ /\bfontMono\b/g) {
            print "$file:" . line_of($code, $-[0]) . ": fontMono (use Theme.fontLabel, fontBody, fontAddress or the eyebrow fields; hosts and the fingerprint use fontAddress)\n";
            $bad = 1;
        }
        # theme["fontMono"]: the string is blanked in $code, so look at it
        # with the strings put back.
        my $fm = $code;
        substr($fm, $_->[0], length $_->[1]) = $_->[1] for @strings;
        while ($fm =~ /\[\s*["']fontMono["']\s*\]/g) {
            print "$file:" . line_of($fm, $-[0]) . ": fontMono (use Theme.fontLabel, fontBody, fontAddress or the eyebrow fields; hosts and the fingerprint use fontAddress)\n";
            $bad = 1;
        }
    }

    # Shapes come from Theme roles (controlRadius, cardRadius, ...): outside
    # qml/platform/Theme.qml a radius (radius, topLeftRadius, ... as a
    # binding, a JS assignment or a Binding { property: "radius" }) is a Theme
    # role read or a circle (width / 2, height / 2). A number, or a gu()
    # size, fails (#68).
    if ($file !~ m{(?:^|/)qml/platform/Theme\.qml$}) {
        my $radius_ok = sub {
            my ($v) = @_;
            $v =~ s/^\s+|\s+$//g;
            # A condition choosing between allowed values is allowed.
            my @alts = $v =~ /^[^?]+\?\s*([^:]+?)\s*:\s*(.+)$/ ? ($1, $2) : ($v);
            for my $alt (@alts) {
                # Circle forms: width / 2, parent.height / 2, width / 2 - 1,
                # Math.min(width, height) / 2.
                next if $alt =~ m{^(?:[\w.]*\b(?:width|height)|Math\.min\(\s*[\w.]+\s*,\s*[\w.]+\s*\))\s*/\s*2(?:\s*-\s*\d+(?:\.\d+)?)?$};
                next if $alt =~ /\btheme\.\w*[Rr]adius\b/ && $alt !~ /\bgu\s*\(/ && $alt !~ /(?<![\w.])\d/;
                return 0;
            }
            return 1;
        };
        my @found;
        # $code with the strings put back (a Binding's property name is one).
        my $nc = $code;
        substr($nc, $_->[0], length $_->[1]) = $_->[1] for @strings;
        # Bindings (and property declarations): `radius: v`, `topLeftRadius: v`.
        while ($code =~ /(?<![\w.])\w*[Rr]adius\s*:\s*([^\n;}]*)/g) {
            push @found, [$-[0], $1];
        }
        # JS assignments: `item.radius = v`, `radius += v` (a compound
        # assignment is never a circle or a role, so it always fails). A JS
        # local (`const radius = 4`) is not a radius property.
        while ($code =~ /(?:\.|(?<![\w.]))\w*[Rr]adius\s*((?:[-+*\/%]|\?\?|\|\||&&)?)=(?![=>])\s*([^\n;}]*)/g) {
            my ($op, $v, $off) = ($1, $2, $-[0]);
            next if substr($code, 0, $off) =~ /\b(?:const|let|var)\s+$/;
            push @found, [$off, $op ne '' ? "$op= $v" : $v];
        }
        # Bracket assignments: `item["radius"] = v`.
        while ($nc =~ /\[\s*["']\w*[Rr]adius["']\s*\]\s*([-+*\/%]?)=(?![=>])\s*([^\n;}]*)/g) {
            my ($op, $v, $off) = ($1, $2, $-[0]);
            push @found, [$off, $op ne '' ? "$op= $v" : $v];
        }
        # Binding { property: "radius"; value: v }, in either order.
        while ($nc =~ /\bBinding\s*\{([^}]*)\}/g) {
            my ($body, $off) = ($1, $-[0]);
            next unless $body =~ /property\s*:\s*["']\w*[Rr]adius["']/;
            push @found, [$off, $body =~ /\bvalue\s*:\s*([^\n;}]*)/ ? $1 : ''];
        }
        for my $f (sort { $a->[0] <=> $b->[0] } @found) {
            my ($off, $v) = @$f;
            $v =~ s/\s+$//;
            next if $radius_ok->($v);
            print "$file:" . line_of($src, $off) . ": radius '$v' (use a Theme radius role, or width / 2 for a circle)\n";
            $bad = 1;
        }
    }

    # Swipe rows: no SwipeDelegate, leadingActions or trailingActions.
    while ($code =~ /\b(SwipeDelegate|leadingActions|trailingActions)\b/g) {
        print "$file:" . line_of($code, $-[0]) . ": $1 (no swipe actions on rows)\n";
        $bad = 1;
    }
}
exit $bad;
