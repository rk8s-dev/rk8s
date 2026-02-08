import numpy as np
from scipy.io import loadmat
import sys
import os

def load_data(path):
    data=loadmat(path)
    X=data["X"]
    y=data["y"]
    return X,y

def sigmoid(z):
    return 1 / (1 + np.exp(-z))

def predict(X,All_Theta):
    h=sigmoid(X@All_Theta.T)
    h_argmax=np.argmax(h,axis=1)
    h_argmax=h_argmax+1
    return h_argmax

def main():
    np.set_printoptions(threshold=sys.maxsize)

    argv = sys.argv
    X, y = load_data(argv[1])
    X=np.insert(X,0,1,axis=1)
    y=y.flatten()
    
    # Load theta values for each class from the provided files
    All_Theta = np.zeros((10, X.shape[1]))  # 10 classes, with the same number of features

    for i in range(0, 10):
        # Load theta for class (i+1) from the file
        file = argv[2+i]
        j = int(file[6])
        with open(file, "r") as f:
            s = f.read().strip("[] ")
            All_Theta[j] = np.fromstring(s, sep=" ", dtype=float)
        os.remove(file)

    # Predict the class using the provided theta values
    y_predict = predict(X, All_Theta)

    # Calculate accuracy
    y_predict=predict(X, All_Theta)
    accuracy=np.mean(y_predict==y)

    print(f"Accuracy: {accuracy * 100:.2f}%\n")

if __name__ == "__main__":
    main()