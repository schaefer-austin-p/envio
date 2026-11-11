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
        } elsif ($c eq '"' || $c eq "'") {
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
            push @strings, [$start, substr($src, $start, $i - $start)];
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
    # Label (Controls 2) and a namespaced X.Text are Texts too.
    while ($code =~ /(?<![\w.])(?:[A-Za-z_]\w*\.)*(Text|Label)\s*\{/g) {
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
        print "$file:" . line_of($code, $open) . ": $type without textFormat (use Text.PlainText)\n";
        $bad = 1;
    }
    my @lines = split /\n/, $src, -1;
    while ($code =~ /\bText\.(AutoText|StyledText|MarkdownText)\b/g) {
        my ($kind, $ln) = ($1, line_of($code, $-[0]));
        next if $kind eq 'StyledText'
            && (($lines[$ln - 1] =~ /styled-ok/) || ($ln > 1 && $lines[$ln - 2] =~ /styled-ok/));
        print "$file:$ln: Text.$kind (use Text.PlainText, or Text.RichText for a sanitised body)\n";
        $bad = 1;
    }

    # Swipe rows: no SwipeDelegate, leadingActions or trailingActions.
    while ($code =~ /\b(SwipeDelegate|leadingActions|trailingActions)\b/g) {
        print "$file:" . line_of($code, $-[0]) . ": $1 (no swipe actions on rows)\n";
        $bad = 1;
    }
}
exit $bad;
