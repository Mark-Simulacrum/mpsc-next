
set ylabel "Throughput (messages/millisecond)"
set xlabel "Sender Threads"

set datafile separator comma

set linetype 1 lc rgb 'pink' pt 7 ps 0.5
set linetype 2 lc rgb '#c2d4ff' pt 7 ps 0.5
set linetype 3 lc rgb 'red' pt 7 ps 0.75

set key autotitle columnhead
set key outside

set terminal svg size 1400,750
set output 'plot.svg'

# 'initial/alt-before-all.csv' using 1:2 lt 5,
plot \
         'initial/alt.csv' using 1:2 lt 1, \
         'initial/std.csv' using 1:2 lt 2,
         #'alt.csv' using 1:2 lt 3
