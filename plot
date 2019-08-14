
set ylabel "Throughput (messages/millisecond)"
set xlabel "Sender Threads"

set datafile separator comma

set linetype 1 lc rgb 'red' pt 7 ps 0.5
set linetype 2 lc rgb 'blue' pt 7 ps 0.5
set linetype 3 lc rgb 'green' pt 7 ps 0.5

set key autotitle columnhead
set key outside

set terminal svg size 1400,750
set output 'unbounded.svg'

plot \
         'alt-unbounded' using 1:2 lt 1, \
         'std-unbounded' using 1:2 lt 2, \

set output 'rendezvous.svg'

plot \
         'alt-rendezvous' using 1:2 lt 1, \
         'std-rendezvous' using 1:2 lt 2
