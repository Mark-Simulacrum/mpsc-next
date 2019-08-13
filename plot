
set ylabel "Throughput (messages/millisecond)"
set xlabel "Sender Threads"

set datafile separator comma

set linetype 1 lc rgb 'red' pt 7 ps 0.5
set linetype 2 lc rgb 'blue' pt 7 ps 0.5

set key autotitle columnhead
set key outside

set terminal svg size 1400,750
set output 'unbounded.svg'

# 'initial/alt-before-all.csv' using 1:2 lt 5,
plot \
         'initial/alt.csv' using 1:2 lt 1, \
         'initial/std.csv' using 1:2 lt 2, \

set output 'rendezvous.svg'

plot \
         'alt-rendezvous' using 1:2 lt 1, \
         'std-rendezvous' using 1:2 lt 2
