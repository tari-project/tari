#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Created on Wed Sep 16 09:12:39 2020

@author: Philip Robinson
"""
import sys;
import matplotlib.pyplot as plt
import networkx as nx
import glob
from matplotlib import animation

import os;

if len(sys.argv) != 5:
    print("Usage: <Dot file directory> <Output directory>, <Plot all connections>, <Plot neighbour connections>")
    exit(1)

print(sys.argv);

dot_path = sys.argv[1];
output_path = sys.argv[2];
plot_connections = sys.argv[3].lower() == 'true';
plot_neighbours = sys.argv[4].lower() == 'true';

if (not plot_neighbours) and (not plot_connections):
    print("Must plot at least 1 graph type");
    exit(5)

print("Input Dot file directory: ", dot_path);
print("Output directory: ", output_path)

#Clear output directory
if os.path.exists(output_path):
    output_files = glob.glob(os.path.join(output_path,'*'));
    for file in output_files:
        os.remove(file)
else:
    os.makedirs(output_path)

neighbour_files = []
if plot_neighbours:
    neighbour_files = glob.glob(os.path.join(dot_path,"neighbours*"));
    neighbour_files.sort()
    if len(neighbour_files) == 0:
        print("No files to process")
        exit(2)

connections_files = []
if plot_connections:
    connections_files = glob.glob(os.path.join(dot_path,"connections*"));
    connections_files.sort()
    if len(connections_files) == 0:
        print("No files to process")
        exit(2)

if plot_connections and plot_neighbours:
    if len(connections_files) != len(neighbour_files):
        print("Number of connection files and neighbour files must be the same")
        exit(4)

# Build plot
if plot_neighbours:
    initial_file = neighbour_files[0]
    num_files = len(neighbour_files);
else:
    num_files = len(connections_files);
    initial_file = connections_files[0]

#Setup initial figure and calculate colour map based on number of nodes
fig, ax = plt.subplots(figsize=(15,10))
G = nx.drawing.nx_pydot.read_dot(initial_file)
G = nx.MultiDiGraph(G.to_directed(), directed=True)
colour_map = plt.cm.get_cmap('hsv', G.number_of_nodes());

for i in range(0, num_files):
    print("Rendering " + output_path + "/" + "{:03}.png".format(i))

    # favour the neighbours for the spring layout otherwise we will use full connections
    if plot_neighbours:
        neighbour_G_dot = nx.drawing.nx_pydot.read_dot(neighbour_files[i])
        pos = nx.spring_layout(neighbour_G_dot)
        neighbour_G = nx.MultiDiGraph(neighbour_G_dot.to_directed(), directed=True)
        nx.draw_networkx_nodes(neighbour_G, pos, cmap=colour_map, node_color=range(0,neighbour_G.number_of_nodes()))
        nx.draw_networkx_labels(neighbour_G, pos, font_weight='bold')
    if plot_connections:
        connections_G_dot = nx.drawing.nx_pydot.read_dot(connections_files[i])
        connections_G = nx.MultiDiGraph(connections_G_dot)
        if not plot_neighbours:
            pos = nx.spring_layout(connections_G)
            nx.draw_networkx_nodes(connections_G, pos, cmap=colour_map, node_color=range(0,connections_G.number_of_nodes()))
            nx.draw_networkx_labels(connections_G, pos, font_weight='bold')

    if plot_neighbours:
        if plot_connections:
            nx.draw_networkx_edges(connections_G, pos, width=2, edge_color='Grey', alpha=0.3, arrows=False)
        nx.draw_networkx_edges(neighbour_G, pos, arrows=True)
    elif plot_connections:
        nx.draw_networkx_edges(connections_G, pos)

    plt.axis('off')
    plt.savefig(output_path + "/" + "{:03}".format(i) + ".png", bbox_inches='tight', dpi=200)
    plt.clf()

exit(0)

